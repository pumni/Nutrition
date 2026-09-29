use application::{
    ApplicationError, FoodEvidenceProvider, ParsedMealItem, ResolvedFoodEvidence,
    ensure_modifiers_represented, normalize_vi_search_key,
};
use async_trait::async_trait;
use domain::{
    CatalogReleaseId, CompositionProfileId, CompositionSnapshot, CompositionValue, EvidenceQuality,
    FoodId, NutrientCode, NutrientUnit, ValueStatus,
};
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use uuid::Uuid;

const EXACT_FOOD_QUERY: &str = r"
    SELECT
        food.id AS food_id,
        food_name.name AS food_name,
        profile.id AS profile_id,
        profile.basis_amount,
        profile.quality_grade,
        nutrient.code AS nutrient_code,
        nutrient.canonical_unit,
        COALESCE(value.canonical_amount, value.amount) AS amount,
        value.minimum_amount,
        value.maximum_amount,
        value.value_status
    FROM catalog.food_name food_name
    JOIN catalog.food_entity food
      ON food.id = food_name.food_id
    JOIN catalog.catalog_release_food_name release_food_name
      ON release_food_name.catalog_release_id = $3
     AND release_food_name.food_name_id = food_name.id
    JOIN LATERAL (
        SELECT candidate.*
        FROM composition.composition_profile candidate
        JOIN catalog.catalog_release_profile release_profile
          ON release_profile.profile_id = candidate.id
         AND release_profile.catalog_release_id = $3
        WHERE candidate.food_id = food.id
          AND candidate.status = 'published'
          AND candidate.basis_unit = 'g'
          AND candidate.edible_basis
        ORDER BY
            CASE candidate.quality_grade
                WHEN 'A' THEN 1
                WHEN 'B' THEN 2
                WHEN 'C' THEN 3
                WHEN 'D' THEN 4
                ELSE 5
            END,
            candidate.created_at DESC,
            candidate.id
        LIMIT 1
    ) profile ON true
    JOIN composition.composition_value value
      ON value.profile_id = profile.id
    JOIN composition.nutrient nutrient
      ON nutrient.id = value.nutrient_id
    WHERE food_name.normalized_name = $1
      AND food_name.locale = $2
      AND food_name.valid_to IS NULL
      AND food.lifecycle_status = 'active'
    ORDER BY food.id, nutrient.code
";

#[derive(Clone)]
pub struct PostgresCatalogEvidenceProvider {
    pool: PgPool,
    catalog_release_id: CatalogReleaseId,
}

impl PostgresCatalogEvidenceProvider {
    #[must_use]
    pub fn new(pool: PgPool, catalog_release_id: CatalogReleaseId) -> Self {
        Self {
            pool,
            catalog_release_id,
        }
    }
}

/// Returns the single active catalog release pinned for new analyses.
///
/// # Errors
///
/// Returns [`crate::PersistenceError::Query`] when the active release is missing, not unique, or
/// cannot be read from `PostgreSQL`.
pub async fn active_catalog_release_id(
    pool: &PgPool,
) -> Result<CatalogReleaseId, crate::PersistenceError> {
    let release_id: Uuid =
        sqlx::query_scalar("SELECT id FROM catalog.catalog_release WHERE status = 'active'")
            .fetch_one(pool)
            .await
            .map_err(crate::PersistenceError::Query)?;
    Ok(CatalogReleaseId::from_uuid(release_id))
}

#[async_trait]
impl FoodEvidenceProvider for PostgresCatalogEvidenceProvider {
    async fn resolve_food(
        &self,
        locale: &str,
        item: &ParsedMealItem,
    ) -> Result<ResolvedFoodEvidence, ApplicationError> {
        ensure_modifiers_represented(&item.food_phrase, &item.modifiers)?;
        let normalized_name = normalize_vi_search_key(&item.food_phrase);
        let rows = sqlx::query(EXACT_FOOD_QUERY)
            .bind(normalized_name)
            .bind(locale)
            .bind(self.catalog_release_id.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(|_| ApplicationError::Persistence)?;

        let food_ids = rows
            .iter()
            .map(|row| {
                row.try_get("food_id")
                    .map_err(|_| ApplicationError::Persistence)
            })
            .collect::<Result<Vec<Uuid>, _>>()?;
        let first_food_id = unique_exact_food_id(&food_ids, &item.food_phrase)?;

        let first = &rows[0];
        let profile_id: Uuid = first
            .try_get("profile_id")
            .map_err(|_| ApplicationError::Persistence)?;
        let basis_g: Decimal = first
            .try_get("basis_amount")
            .map_err(|_| ApplicationError::Persistence)?;
        let quality = parse_quality(
            first
                .try_get("quality_grade")
                .map_err(|_| ApplicationError::Persistence)?,
        )?;
        let values = rows
            .iter()
            .map(row_to_composition_value)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ResolvedFoodEvidence {
            food_id: FoodId::from_uuid(first_food_id),
            food_name: first
                .try_get("food_name")
                .map_err(|_| ApplicationError::Persistence)?,
            composition: CompositionSnapshot {
                profile_id: CompositionProfileId::from_uuid(profile_id),
                basis_g,
                quality,
                values,
            },
            quality,
        })
    }
}

fn unique_exact_food_id(food_ids: &[Uuid], food_phrase: &str) -> Result<Uuid, ApplicationError> {
    let Some(first_food_id) = food_ids.first().copied() else {
        return Err(ApplicationError::InsufficientEvidence(format!(
            "unknown exact food: {food_phrase}"
        )));
    };
    if food_ids.iter().any(|food_id| *food_id != first_food_id) {
        return Err(ApplicationError::InsufficientEvidence(format!(
            "ambiguous exact food: {food_phrase}"
        )));
    }
    Ok(first_food_id)
}

fn row_to_composition_value(
    row: &sqlx::postgres::PgRow,
) -> Result<CompositionValue, ApplicationError> {
    let nutrient_code: String = row
        .try_get("nutrient_code")
        .map_err(|_| ApplicationError::Persistence)?;
    let canonical_unit: String = row
        .try_get("canonical_unit")
        .map_err(|_| ApplicationError::Persistence)?;
    let value_status: String = row
        .try_get("value_status")
        .map_err(|_| ApplicationError::Persistence)?;
    Ok(CompositionValue {
        nutrient: NutrientCode::new(nutrient_code).map_err(|_| ApplicationError::Persistence)?,
        amount: row
            .try_get("amount")
            .map_err(|_| ApplicationError::Persistence)?,
        lower_amount: row
            .try_get("minimum_amount")
            .map_err(|_| ApplicationError::Persistence)?,
        upper_amount: row
            .try_get("maximum_amount")
            .map_err(|_| ApplicationError::Persistence)?,
        unit: parse_unit(&canonical_unit)?,
        status: parse_value_status(&value_status)?,
    })
}

pub(crate) fn parse_quality(value: &str) -> Result<EvidenceQuality, ApplicationError> {
    match value {
        "A" => Ok(EvidenceQuality::A),
        "B" => Ok(EvidenceQuality::B),
        "C" => Ok(EvidenceQuality::C),
        "D" => Ok(EvidenceQuality::D),
        "U" => Ok(EvidenceQuality::U),
        _ => Err(ApplicationError::Persistence),
    }
}

fn parse_unit(value: &str) -> Result<NutrientUnit, ApplicationError> {
    match value {
        "kcal" => Ok(NutrientUnit::Kilocalorie),
        "g" => Ok(NutrientUnit::Gram),
        "mg" => Ok(NutrientUnit::Milligram),
        "ug" => Ok(NutrientUnit::Microgram),
        _ => Err(ApplicationError::Persistence),
    }
}

fn parse_value_status(value: &str) -> Result<ValueStatus, ApplicationError> {
    match value {
        "measured" => Ok(ValueStatus::Measured),
        "declared" => Ok(ValueStatus::Declared),
        "calculated" => Ok(ValueStatus::Calculated),
        "compiled" => Ok(ValueStatus::Compiled),
        "estimated" => Ok(ValueStatus::Estimated),
        "trace" => Ok(ValueStatus::Trace),
        "not_detected" => Ok(ValueStatus::NotDetected),
        "missing" => Ok(ValueStatus::Missing),
        _ => Err(ApplicationError::Persistence),
    }
}

#[cfg(test)]
mod tests {
    use super::unique_exact_food_id;
    use application::ApplicationError;
    use uuid::Uuid;

    #[test]
    fn allows_exact_rows_for_one_food_and_rejects_unknown_or_ambiguous_foods() {
        let supported_food_id = Uuid::from_u128(1);
        assert_eq!(
            unique_exact_food_id(&[supported_food_id, supported_food_id], "trứng gà luộc")
                .expect("rows for one exact food remain resolvable"),
            supported_food_id
        );

        assert!(matches!(
            unique_exact_food_id(&[], "món không tồn tại"),
            Err(ApplicationError::InsufficientEvidence(_))
        ));
        assert!(matches!(
            unique_exact_food_id(&[Uuid::from_u128(1), Uuid::from_u128(2)], "mơ hồ"),
            Err(ApplicationError::InsufficientEvidence(_))
        ));
    }
}
