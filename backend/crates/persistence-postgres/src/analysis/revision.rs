//! Append-only revision and finalization responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) async fn load_nutrient_ids(
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

pub(crate) async fn insert_analysis(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
) -> Result<(), ApplicationError> {
    sqlx::query(
        r"
        INSERT INTO analysis.meal_analysis (
            id, user_id, locale, idempotency_key, status
        ) VALUES ($1, $2, $3, $4, 'resolving')
        ",
    )
    .bind(snapshot.analysis_id.as_uuid())
    .bind(snapshot.owner_id.map(UserId::as_uuid))
    .bind(&snapshot.locale)
    .bind(
        snapshot
            .idempotency
            .as_ref()
            .map(|context| context.key.as_str()),
    )
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    Ok(())
}

pub(crate) async fn insert_revision(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
    revision_reason: &str,
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
            clarification_policy_version,
            correction_policy_version,
            calculation_engine_version,
            catalog_release_id,
            result_status,
            quality_label,
            assumptions,
            warnings
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, 'building', $17, $18, $19
        )
        ",
    )
    .bind(snapshot.revision_id.as_uuid())
    .bind(snapshot.analysis_id.as_uuid())
    .bind(i32::try_from(snapshot.revision_number).map_err(|_| ApplicationError::Persistence)?)
    .bind(revision_reason)
    .bind(&versions.application_version)
    .bind(&versions.parser_schema_version)
    .bind(&versions.prompt_version)
    .bind(&versions.model_provider_version)
    .bind(&versions.normalization_version)
    .bind(&versions.resolution_policy_version)
    .bind(&versions.portion_policy_version)
    .bind(&versions.composition_policy_version)
    .bind(&versions.clarification_policy_version)
    .bind(&versions.correction_policy_version)
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

pub(crate) async fn insert_clarification_revision(
    transaction: &mut Transaction<'_, Postgres>,
    clarification: &ClarificationAnalysis,
) -> Result<(), ApplicationError> {
    let versions = &clarification.versions;
    sqlx::query(
        r"
        INSERT INTO analysis.analysis_revision (
            id, meal_analysis_id, revision_number, revision_reason,
            application_version, parser_schema_version, prompt_version,
            model_provider_version, normalization_version, resolution_policy_version,
            portion_policy_version, composition_policy_version, clarification_policy_version,
            correction_policy_version, calculation_engine_version, catalog_release_id,
            result_status, quality_label, assumptions, warnings
        ) VALUES (
            $1, $2, $3, 'portion_clarification_required',
            $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            'building', 'insufficient', '[]', '[]'
        )
        ",
    )
    .bind(clarification.revision_id.as_uuid())
    .bind(clarification.analysis_id.as_uuid())
    .bind(i32::try_from(clarification.revision_number).map_err(|_| ApplicationError::Persistence)?)
    .bind(&versions.application_version)
    .bind(&versions.parser_schema_version)
    .bind(&versions.prompt_version)
    .bind(&versions.model_provider_version)
    .bind(&versions.normalization_version)
    .bind(&versions.resolution_policy_version)
    .bind(&versions.portion_policy_version)
    .bind(&versions.composition_policy_version)
    .bind(&versions.clarification_policy_version)
    .bind(&versions.correction_policy_version)
    .bind(&versions.calculation_engine_version)
    .bind(versions.catalog_release_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    Ok(())
}

pub(crate) async fn lock_current_revision(
    transaction: &mut Transaction<'_, Postgres>,
    analysis_id: AnalysisId,
    expected_revision_id: AnalysisRevisionId,
    expected_status: &str,
    conflict: ApplicationError,
) -> Result<(), ApplicationError> {
    let row = sqlx::query(
        "SELECT current_revision_id, status
           FROM analysis.meal_analysis
          WHERE id = $1
          FOR UPDATE",
    )
    .bind(analysis_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?
    .ok_or(ApplicationError::NotFound)?;
    let current_revision_id: Uuid = row
        .try_get("current_revision_id")
        .map_err(|_| ApplicationError::Persistence)?;
    let status: String = row
        .try_get("status")
        .map_err(|_| ApplicationError::Persistence)?;
    if current_revision_id != expected_revision_id.as_uuid() || status != expected_status {
        return Err(conflict);
    }
    Ok(())
}

pub(crate) async fn transition_status(
    transaction: &mut Transaction<'_, Postgres>,
    analysis_id: AnalysisId,
    from: &str,
    to: &str,
) -> Result<(), ApplicationError> {
    let changed = sqlx::query(
        "UPDATE analysis.meal_analysis
            SET status = $3
          WHERE id = $1
            AND status = $2",
    )
    .bind(analysis_id.as_uuid())
    .bind(from)
    .bind(to)
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    if changed.rows_affected() != 1 {
        return Err(ApplicationError::Persistence);
    }
    Ok(())
}

pub(crate) async fn set_completed_from(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &AnalysisSnapshot,
    from: &str,
) -> Result<(), ApplicationError> {
    let changed = sqlx::query(
        "UPDATE analysis.meal_analysis
            SET status = 'completed',
                current_revision_id = $2
          WHERE id = $1
            AND status = $3",
    )
    .bind(snapshot.analysis_id.as_uuid())
    .bind(snapshot.revision_id.as_uuid())
    .bind(from)
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    if changed.rows_affected() != 1 {
        return Err(ApplicationError::Persistence);
    }
    Ok(())
}

pub(crate) async fn insert_items_and_results(
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
                lower_mass_g,
                upper_mass_g,
                evidence_quality
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
            )
            ",
        )
        .bind(item_id.as_uuid())
        .bind(snapshot.revision_id.as_uuid())
        .bind(i32::try_from(index).map_err(|_| ApplicationError::Persistence)?)
        .bind("[redacted]")
        .bind(json!({
            "mass_resolution_method": mass_method_code(item.mass_resolution_method)
        }))
        .bind(resolution_status_code(item.mass_resolution_method))
        .bind(item.food_id.as_uuid())
        .bind(item.profile_id.as_uuid())
        .bind(
            item.portion_observation_id
                .map(domain::PortionObservationId::as_uuid),
        )
        .bind(item.estimated_mass_g)
        .bind(item.lower_mass_g)
        .bind(item.upper_mass_g)
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

pub(crate) async fn insert_totals(
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

pub(crate) async fn finalize_revision(
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

pub(crate) async fn finalize_revision_value(
    transaction: &mut Transaction<'_, Postgres>,
    revision_id: AnalysisRevisionId,
    snapshot_value: Value,
    snapshot_hash: &str,
) -> Result<(), ApplicationError> {
    let changed = sqlx::query(
        "UPDATE analysis.analysis_revision
            SET result_status = 'completed',
                result_snapshot = $2,
                snapshot_hash = $3
          WHERE id = $1
            AND result_status = 'building'",
    )
    .bind(revision_id.as_uuid())
    .bind(snapshot_value)
    .bind(snapshot_hash)
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    if changed.rows_affected() != 1 {
        return Err(ApplicationError::Persistence);
    }
    Ok(())
}

pub(crate) async fn finalize_analysis(
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

pub(crate) async fn insert_outbox(
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

pub(crate) async fn insert_workflow_outbox(
    transaction: &mut Transaction<'_, Postgres>,
    analysis_id: AnalysisId,
    revision_id: AnalysisRevisionId,
    event_type: &str,
) -> Result<(), ApplicationError> {
    sqlx::query(
        "INSERT INTO ops.outbox_event (
            id, aggregate_type, aggregate_id, event_type, payload
         ) VALUES ($1, 'meal_analysis', $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(analysis_id.as_uuid())
    .bind(event_type)
    .bind(json!({
        "analysis_id": analysis_id,
        "revision_id": revision_id
    }))
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    Ok(())
}
