use application::{AnalysisRepository, AnalysisSnapshot, AnalysisSnapshotReader, ApplicationError};
use async_trait::async_trait;
use domain::{
    AnalysisId, AnalysisItemId, EvidenceQuality, MassResolutionMethod, NutrientCode, NutrientUnit,
    ValueStatus,
};
use hex::encode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresAnalysisRepository {
    pool: PgPool,
}

impl PostgresAnalysisRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AnalysisRepository for PostgresAnalysisRepository {
    async fn save(&self, snapshot: &AnalysisSnapshot) -> Result<(), ApplicationError> {
        persist_snapshot(&self.pool, snapshot).await
    }
}

#[async_trait]
impl AnalysisSnapshotReader for PostgresAnalysisRepository {
    async fn find(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisSnapshot>, ApplicationError> {
        let row = sqlx::query(
            r"
            SELECT revision.result_snapshot, revision.snapshot_hash
            FROM analysis.meal_analysis meal
            JOIN analysis.analysis_revision revision
              ON revision.id = meal.current_revision_id
            WHERE meal.id = $1
            ",
        )
        .bind(analysis_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ApplicationError::Persistence)?;

        let Some(row) = row else {
            return Ok(None);
        };
        let snapshot_value: Value = row
            .try_get("result_snapshot")
            .map_err(|_| ApplicationError::Persistence)?;
        let expected_hash: String = row
            .try_get("snapshot_hash")
            .map_err(|_| ApplicationError::Persistence)?;
        let encoded =
            serde_json::to_vec(&snapshot_value).map_err(|_| ApplicationError::Persistence)?;
        if sha256_hex(&encoded) != expected_hash {
            return Err(ApplicationError::Persistence);
        }
        serde_json::from_value(snapshot_value)
            .map(Some)
            .map_err(|_| ApplicationError::Persistence)
    }
}

async fn persist_snapshot(
    pool: &PgPool,
    snapshot: &AnalysisSnapshot,
) -> Result<(), ApplicationError> {
    let snapshot_value =
        serde_json::to_value(snapshot).map_err(|_| ApplicationError::Persistence)?;
    let snapshot_bytes =
        serde_json::to_vec(&snapshot_value).map_err(|_| ApplicationError::Persistence)?;
    let snapshot_hash = sha256_hex(&snapshot_bytes);
    let nutrient_ids = load_nutrient_ids(pool, &snapshot.requested_nutrients).await?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| ApplicationError::Persistence)?;

    insert_analysis(&mut transaction, snapshot).await?;
    insert_revision(&mut transaction, snapshot).await?;
    insert_items_and_results(&mut transaction, snapshot, &nutrient_ids).await?;
    insert_totals(&mut transaction, snapshot, &nutrient_ids).await?;
    finalize_revision(&mut transaction, snapshot, snapshot_value, &snapshot_hash).await?;
    finalize_analysis(&mut transaction, snapshot).await?;
    insert_outbox(&mut transaction, snapshot).await?;

    transaction
        .commit()
        .await
        .map_err(|_| ApplicationError::Persistence)
}

async fn load_nutrient_ids(
    pool: &PgPool,
    nutrients: &[NutrientCode],
) -> Result<HashMap<String, Uuid>, ApplicationError> {
    let codes = nutrients
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let rows =
        sqlx::query("SELECT id, code FROM composition.nutrient WHERE code = ANY($1::text[])")
            .bind(&codes)
            .fetch_all(pool)
            .await
            .map_err(|_| ApplicationError::Persistence)?;
    let mut result = HashMap::with_capacity(rows.len());
    for row in rows {
        result.insert(
            row.try_get("code")
                .map_err(|_| ApplicationError::Persistence)?,
            row.try_get("id")
                .map_err(|_| ApplicationError::Persistence)?,
        );
    }
    if codes.iter().any(|code| !result.contains_key(code)) {
        return Err(ApplicationError::Persistence);
    }
    Ok(result)
}

async fn insert_analysis(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
) -> Result<(), ApplicationError> {
    sqlx::query(
        r"
        INSERT INTO analysis.meal_analysis (
            id, locale, status
        ) VALUES ($1, $2, 'resolving')
        ",
    )
    .bind(snapshot.analysis_id.as_uuid())
    .bind(&snapshot.locale)
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    Ok(())
}

async fn insert_revision(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
) -> Result<(), ApplicationError> {
    let versions = &snapshot.versions;
    sqlx::query(
        r"
        INSERT INTO analysis.analysis_revision (
            id,
            meal_analysis_id,
            revision_number,
            revision_reason,
            application_version,
            parser_schema_version,
            prompt_version,
            model_provider_version,
            normalization_version,
            resolution_policy_version,
            portion_policy_version,
            composition_policy_version,
            calculation_engine_version,
            catalog_release_id,
            result_status,
            quality_label,
            assumptions,
            warnings
        ) VALUES (
            $1, $2, $3, 'initial_analysis', $4, $5, $6, $7, $8, $9,
            $10, $11, $12, $13, 'building', $14, $15, $16
        )
        ",
    )
    .bind(snapshot.revision_id.as_uuid())
    .bind(snapshot.analysis_id.as_uuid())
    .bind(i32::try_from(snapshot.revision_number).map_err(|_| ApplicationError::Persistence)?)
    .bind(&versions.application_version)
    .bind(&versions.parser_schema_version)
    .bind(&versions.prompt_version)
    .bind(&versions.model_provider_version)
    .bind(&versions.normalization_version)
    .bind(&versions.resolution_policy_version)
    .bind(&versions.portion_policy_version)
    .bind(&versions.composition_policy_version)
    .bind(&versions.calculation_engine_version)
    .bind(versions.catalog_release_id.as_uuid())
    .bind(overall_quality(snapshot))
    .bind(json!(
        snapshot
            .items
            .iter()
            .flat_map(|item| item.assumptions.iter())
            .collect::<Vec<_>>()
    ))
    .bind(json!(snapshot.calculation.warnings))
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    Ok(())
}

async fn insert_items_and_results(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
    nutrient_ids: &HashMap<String, Uuid>,
) -> Result<(), ApplicationError> {
    if snapshot.items.len() != snapshot.calculation.items.len() {
        return Err(ApplicationError::Persistence);
    }

    for (index, (item, result)) in snapshot
        .items
        .iter()
        .zip(&snapshot.calculation.items)
        .enumerate()
    {
        if item.food_id != result.food_id {
            return Err(ApplicationError::Persistence);
        }
        let item_id = AnalysisItemId::new();
        sqlx::query(
            r"
            INSERT INTO analysis.analysis_item (
                id,
                revision_id,
                item_index,
                source_text,
                parsed_payload,
                resolution_status,
                resolved_food_id,
                resolved_profile_id,
                resolved_portion_observation_id,
                estimated_mass_g,
                evidence_quality
            ) VALUES (
                $1, $2, $3, $4, $5, 'resolved_exact', $6, $7, $8, $9, $10
            )
            ",
        )
        .bind(item_id.as_uuid())
        .bind(snapshot.revision_id.as_uuid())
        .bind(i32::try_from(index).map_err(|_| ApplicationError::Persistence)?)
        .bind(&item.source_text)
        .bind(json!({
            "mass_resolution_method": mass_method_code(item.mass_resolution_method)
        }))
        .bind(item.food_id.as_uuid())
        .bind(item.profile_id.as_uuid())
        .bind(
            item.portion_observation_id
                .map(domain::PortionObservationId::as_uuid),
        )
        .bind(item.estimated_mass_g)
        .bind(quality_code(item.evidence_quality))
        .execute(&mut **transaction)
        .await
        .map_err(|_| ApplicationError::Persistence)?;

        for nutrient in &result.nutrients {
            let nutrient_id = nutrient_ids
                .get(nutrient.nutrient.as_str())
                .ok_or(ApplicationError::Persistence)?;
            sqlx::query(
                r"
                INSERT INTO analysis.item_nutrient_result (
                    item_id,
                    nutrient_id,
                    amount,
                    lower_amount,
                    upper_amount,
                    unit,
                    status,
                    calculation_trace
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ",
            )
            .bind(item_id.as_uuid())
            .bind(nutrient_id)
            .bind(nutrient.amount)
            .bind(nutrient.lower_amount)
            .bind(nutrient.upper_amount)
            .bind(unit_code(nutrient.unit))
            .bind(value_status_code(nutrient.source_status))
            .bind(
                nutrient
                    .operation
                    .as_ref()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|_| ApplicationError::Persistence)?,
            )
            .execute(&mut **transaction)
            .await
            .map_err(|_| ApplicationError::Persistence)?;
        }
    }
    Ok(())
}

async fn insert_totals(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
    nutrient_ids: &HashMap<String, Uuid>,
) -> Result<(), ApplicationError> {
    for total in &snapshot.calculation.totals {
        let nutrient_id = nutrient_ids
            .get(total.nutrient.as_str())
            .ok_or(ApplicationError::Persistence)?;
        sqlx::query(
            r"
            INSERT INTO analysis.revision_nutrient_total (
                revision_id,
                nutrient_id,
                amount,
                lower_amount,
                upper_amount,
                unit,
                completeness_ratio
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            ",
        )
        .bind(snapshot.revision_id.as_uuid())
        .bind(nutrient_id)
        .bind(total.amount)
        .bind(total.lower_amount)
        .bind(total.upper_amount)
        .bind(unit_code(total.unit))
        .bind(total.completeness_ratio)
        .execute(&mut **transaction)
        .await
        .map_err(|_| ApplicationError::Persistence)?;
    }
    Ok(())
}

async fn finalize_revision(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
    snapshot_value: Value,
    snapshot_hash: &str,
) -> Result<(), ApplicationError> {
    let result = sqlx::query(
        r"
        UPDATE analysis.analysis_revision
           SET result_status = 'completed',
               result_snapshot = $2,
               snapshot_hash = $3
         WHERE id = $1
           AND result_status = 'building'
        ",
    )
    .bind(snapshot.revision_id.as_uuid())
    .bind(snapshot_value)
    .bind(snapshot_hash)
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    if result.rows_affected() != 1 {
        return Err(ApplicationError::Persistence);
    }
    Ok(())
}

async fn finalize_analysis(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
) -> Result<(), ApplicationError> {
    let result = sqlx::query(
        r"
        UPDATE analysis.meal_analysis
           SET status = 'completed',
               current_revision_id = $2
         WHERE id = $1
           AND status = 'resolving'
        ",
    )
    .bind(snapshot.analysis_id.as_uuid())
    .bind(snapshot.revision_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    if result.rows_affected() != 1 {
        return Err(ApplicationError::Persistence);
    }
    Ok(())
}

async fn insert_outbox(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
) -> Result<(), ApplicationError> {
    sqlx::query(
        r"
        INSERT INTO ops.outbox_event (
            id, aggregate_type, aggregate_id, event_type, payload
        ) VALUES ($1, 'meal_analysis', $2, 'MealAnalysisCompleted', $3)
        ",
    )
    .bind(Uuid::now_v7())
    .bind(snapshot.analysis_id.as_uuid())
    .bind(json!({
        "analysis_id": snapshot.analysis_id,
        "revision_id": snapshot.revision_id,
        "catalog_release_id": snapshot.versions.catalog_release_id,
        "calculation_engine_version": snapshot.versions.calculation_engine_version
    }))
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    Ok(())
}

fn sha256_hex(value: &[u8]) -> String {
    encode(Sha256::digest(value))
}

fn overall_quality(snapshot: &AnalysisSnapshot) -> &'static str {
    if snapshot.items.iter().all(|item| {
        matches!(
            item.evidence_quality,
            EvidenceQuality::A | EvidenceQuality::B
        )
    }) {
        "high"
    } else if snapshot
        .items
        .iter()
        .any(|item| item.evidence_quality == EvidenceQuality::U)
    {
        "insufficient"
    } else {
        "medium"
    }
}

const fn quality_code(value: EvidenceQuality) -> &'static str {
    match value {
        EvidenceQuality::A => "A",
        EvidenceQuality::B => "B",
        EvidenceQuality::C => "C",
        EvidenceQuality::D => "D",
        EvidenceQuality::U => "U",
    }
}

const fn mass_method_code(value: MassResolutionMethod) -> &'static str {
    match value {
        MassResolutionMethod::ExplicitMass => "explicit_mass",
        MassResolutionMethod::VolumeDensity => "volume_density",
        MassResolutionMethod::BrandedServing => "branded_serving",
        MassResolutionMethod::PortionObservation => "portion_observation",
        MassResolutionMethod::CuratedDefault => "curated_default",
    }
}

const fn unit_code(value: NutrientUnit) -> &'static str {
    match value {
        NutrientUnit::Kilocalorie => "kcal",
        NutrientUnit::Gram => "g",
        NutrientUnit::Milligram => "mg",
        NutrientUnit::Microgram => "ug",
    }
}

const fn value_status_code(value: ValueStatus) -> &'static str {
    match value {
        ValueStatus::Measured => "measured",
        ValueStatus::Declared => "declared",
        ValueStatus::Calculated => "calculated",
        ValueStatus::Compiled => "compiled",
        ValueStatus::Estimated => "estimated",
        ValueStatus::Trace => "trace",
        ValueStatus::NotDetected => "not_detected",
        ValueStatus::Missing => "missing",
    }
}
