use application::{
    ApplicationError, ParsedMealItem, PortionEvidenceProvider, PortionSuggestion,
    ResolvedPortionEvidence, normalize_vi_search_key,
};
use async_trait::async_trait;
use domain::{EvidenceQuality, FoodId, MassEstimate, MassResolutionMethod, PortionObservationId};
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use uuid::Uuid;

const PORTION_OBSERVATION_QUERY: &str = r"
    SELECT
        observation.id,
        observation.measure_amount,
        observation.gram_weight,
        observation.lower_bound_g,
        observation.upper_bound_g,
        observation.quality_grade,
        measure.canonical_label_vi
    FROM composition.measure_unit measure
    JOIN composition.portion_observation observation
      ON observation.measure_unit_id = measure.id
    JOIN catalog.catalog_release active_release
      ON active_release.status = 'active'
    JOIN catalog.catalog_release_portion_observation release_portion
      ON release_portion.catalog_release_id = active_release.id
     AND release_portion.portion_observation_id = observation.id
    WHERE observation.food_id = $1
      AND (
          measure.code = $2
          OR measure.canonical_label_vi = $2
          OR measure.aliases ? $2
      )
      AND observation.valid_to IS NULL
    ORDER BY
        CASE observation.quality_grade
            WHEN 'A' THEN 1
            WHEN 'B' THEN 2
            WHEN 'C' THEN 3
            WHEN 'D' THEN 4
            ELSE 5
        END,
        observation.id
    LIMIT 1
";

#[derive(Clone)]
pub struct PostgresPortionEvidenceProvider {
    pool: PgPool,
}

impl PostgresPortionEvidenceProvider {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PortionEvidenceProvider for PostgresPortionEvidenceProvider {
    async fn resolve_portion(
        &self,
        _locale: &str,
        item: &ParsedMealItem,
        food_id: FoodId,
    ) -> Result<ResolvedPortionEvidence, ApplicationError> {
        let quantity = positive_quantity(item)?;
        let unit = item
            .unit_phrase
            .as_deref()
            .map(normalize_vi_search_key)
            .ok_or_else(|| {
                ApplicationError::InsufficientEvidence("portion unit is required".to_owned())
            })?;
        if unit == "g" {
            return Ok(explicit_mass(quantity));
        }

        let row = sqlx::query(PORTION_OBSERVATION_QUERY)
            .bind(food_id.as_uuid())
            .bind(&unit)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ApplicationError::Persistence)?
            .ok_or_else(|| {
                ApplicationError::InsufficientEvidence(format!(
                    "no contextual portion evidence for unit: {unit}"
                ))
            })?;
        let observation_id: Uuid = row
            .try_get("id")
            .map_err(|_| ApplicationError::Persistence)?;
        let measure_amount: Decimal = row
            .try_get("measure_amount")
            .map_err(|_| ApplicationError::Persistence)?;
        let gram_weight: Decimal = row
            .try_get("gram_weight")
            .map_err(|_| ApplicationError::Persistence)?;
        let lower_bound: Option<Decimal> = row
            .try_get("lower_bound_g")
            .map_err(|_| ApplicationError::Persistence)?;
        let upper_bound: Option<Decimal> = row
            .try_get("upper_bound_g")
            .map_err(|_| ApplicationError::Persistence)?;
        let quality = crate::catalog_repository::parse_quality(
            row.try_get("quality_grade")
                .map_err(|_| ApplicationError::Persistence)?,
        )?;
        let label: String = row
            .try_get("canonical_label_vi")
            .map_err(|_| ApplicationError::Persistence)?;

        Ok(ResolvedPortionEvidence {
            mass: MassEstimate {
                central_g: quantity * gram_weight / measure_amount,
                lower_g: lower_bound.map(|bound| quantity * bound / measure_amount),
                upper_g: upper_bound.map(|bound| quantity * bound / measure_amount),
                evidence_id: Some(PortionObservationId::from_uuid(observation_id)),
                method: MassResolutionMethod::PortionObservation,
            },
            quality,
            assumptions: vec![format!("Sử dụng quan sát khẩu phần cho đơn vị “{label}”")],
        })
    }

    async fn suggestions(
        &self,
        _locale: &str,
        food_id: FoodId,
    ) -> Result<Vec<PortionSuggestion>, ApplicationError> {
        let rows = sqlx::query(
            r"
            SELECT DISTINCT measure.code, measure.canonical_label_vi
            FROM composition.measure_unit measure
            JOIN composition.portion_observation observation
              ON observation.measure_unit_id = measure.id
            JOIN catalog.catalog_release active_release
              ON active_release.status = 'active'
            JOIN catalog.catalog_release_portion_observation membership
              ON membership.catalog_release_id = active_release.id
             AND membership.portion_observation_id = observation.id
            WHERE observation.food_id = $1
              AND observation.valid_to IS NULL
            ORDER BY measure.canonical_label_vi
            LIMIT 3
            ",
        )
        .bind(food_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ApplicationError::Persistence)?;
        rows.into_iter()
            .map(|row| {
                Ok(PortionSuggestion {
                    unit: row
                        .try_get("canonical_label_vi")
                        .map_err(|_| ApplicationError::Persistence)?,
                    label: format!(
                        "Tính theo {}",
                        row.try_get::<String, _>("canonical_label_vi")
                            .map_err(|_| ApplicationError::Persistence)?
                    ),
                })
            })
            .collect()
    }
}

fn positive_quantity(item: &ParsedMealItem) -> Result<Decimal, ApplicationError> {
    let quantity = item.quantity.ok_or_else(|| {
        ApplicationError::InsufficientEvidence("explicit quantity is required".to_owned())
    })?;
    if quantity <= Decimal::ZERO {
        return Err(ApplicationError::InsufficientEvidence(
            "quantity must be positive".to_owned(),
        ));
    }
    Ok(quantity)
}

fn explicit_mass(quantity: Decimal) -> ResolvedPortionEvidence {
    ResolvedPortionEvidence {
        mass: MassEstimate {
            central_g: quantity,
            lower_g: None,
            upper_g: None,
            evidence_id: None,
            method: MassResolutionMethod::ExplicitMass,
        },
        quality: EvidenceQuality::A,
        assumptions: Vec::new(),
    }
}
