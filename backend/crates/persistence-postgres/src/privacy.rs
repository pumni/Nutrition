use domain::UserId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

pub const USER_DATA_EXPORT_VERSION: &str = "user-data-export-v1";
const REDACTED: &str = "[redacted]";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrivacyDeletionReceipt {
    pub event_type: &'static str,
    pub deleted_at: String,
    pub request_reference: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacyRetentionReport {
    pub deleted_parser_telemetry: u64,
    pub deleted_audit_events: u64,
    pub purged_user_aggregates: u64,
}

#[derive(Debug, Error)]
pub enum PrivacyError {
    #[error("privacy operation input is invalid: {0}")]
    InvalidInput(String),
    #[error("privacy operation database query failed")]
    Query(#[from] sqlx::Error),
    #[error("privacy export JSON serialization failed")]
    Json(#[from] serde_json::Error),
}

/// Replaces sensitive fields while preserving the shape required to decode an analysis snapshot.
pub(crate) fn redact_persisted_value(value: &mut Value) {
    redact_value(value, false);
}

/// Removes sensitive fields from a value that is leaving the persistence boundary.
pub(crate) fn redact_export_value(value: &mut Value) {
    redact_value(value, true);
}

fn redact_value(value: &mut Value, remove_fields: bool) {
    match value {
        Value::Array(values) => {
            for value in values {
                redact_value(value, remove_fields);
            }
        }
        Value::Object(object) => {
            let sensitive_keys = object
                .keys()
                .filter(|key| is_sensitive_key(key))
                .cloned()
                .collect::<Vec<_>>();
            for key in sensitive_keys {
                if remove_fields {
                    object.remove(&key);
                } else if key == "modifiers" {
                    object.insert(key, Value::Array(Vec::new()));
                } else {
                    object.insert(key, Value::String(REDACTED.to_owned()));
                }
            }
            for value in object.values_mut() {
                redact_value(value, remove_fields);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key,
        "source_text"
            | "raw_text"
            | "raw_text_ciphertext"
            | "meal_text"
            | "untrusted_meal_text"
            | "source_spans"
            | "food_phrase"
            | "modifiers"
    )
}

/// Builds the versioned, content-redacted export for one user.
///
/// # Errors
///
/// Returns [`PrivacyError`] when the owned data cannot be read or serialized.
pub async fn export_user_data(pool: &PgPool, user_id: UserId) -> Result<Value, PrivacyError> {
    let analysis_rows = sqlx::query(
        "SELECT meal.id::text AS analysis_id,
                meal.status,
                meal.locale,
                meal.created_at::text AS analysis_created_at,
                revision.id::text AS revision_id,
                revision.revision_number,
                revision.revision_reason,
                revision.result_status,
                revision.result_snapshot,
                revision.snapshot_hash,
                revision.application_version,
                revision.parser_schema_version,
                revision.prompt_version,
                revision.model_provider_version,
                revision.normalization_version,
                revision.resolution_policy_version,
                revision.portion_policy_version,
                revision.composition_policy_version,
                revision.clarification_policy_version,
                revision.correction_policy_version,
                revision.calculation_engine_version,
                revision.catalog_release_id::text AS catalog_release_id,
                revision.created_at::text AS revision_created_at
           FROM analysis.meal_analysis meal
           LEFT JOIN analysis.analysis_revision revision
             ON revision.meal_analysis_id = meal.id
          WHERE meal.user_id = $1
          ORDER BY meal.created_at, revision.revision_number",
    )
    .bind(user_id.as_uuid())
    .fetch_all(pool)
    .await?;

    let mut analyses = BTreeMap::<String, Value>::new();
    let mut revisions = Vec::new();
    for row in analysis_rows {
        let analysis_id: String = row.try_get("analysis_id")?;
        analyses.entry(analysis_id.clone()).or_insert_with(|| {
            json!({
                "analysis_id": analysis_id,
                "status": row.get::<String, _>("status"),
                "locale": row.get::<String, _>("locale"),
                "created_at": row.get::<String, _>("analysis_created_at"),
                "revision_ids": []
            })
        });
        let Some(revision_id) = row.try_get::<Option<String>, _>("revision_id")? else {
            continue;
        };
        let mut snapshot = row
            .try_get::<Option<Value>, _>("result_snapshot")?
            .unwrap_or(Value::Null);
        redact_export_value(&mut snapshot);
        let revision = json!({
            "revision_id": revision_id,
            "analysis_id": analysis_id,
            "revision_number": row.try_get::<i32, _>("revision_number")?,
            "revision_reason": row.try_get::<String, _>("revision_reason")?,
            "result_status": row.try_get::<String, _>("result_status")?,
            "result": snapshot,
            "snapshot_hash": row.try_get::<Option<String>, _>("snapshot_hash")?,
            "created_at": row.try_get::<String, _>("revision_created_at")?,
            "behavior_versions": behavior_versions(&row)?
        });
        revisions.push(revision);
        if let Some(Value::Object(analysis)) = analyses.get_mut(&analysis_id) {
            analysis
                .get_mut("revision_ids")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| PrivacyError::InvalidInput("export shape is invalid".to_owned()))?
                .push(Value::String(revision_id));
        }
    }

    let clarifications = export_clarifications(pool, user_id).await?;
    let corrections = export_corrections(pool, user_id).await?;
    let exported_at: String = sqlx::query_scalar("SELECT now()::text")
        .fetch_one(pool)
        .await?;
    Ok(json!({
        "export_version": USER_DATA_EXPORT_VERSION,
        "user_id": user_id,
        "exported_at": exported_at,
        "analyses": analyses.into_values().collect::<Vec<_>>(),
        "revisions": revisions,
        "clarifications": clarifications,
        "corrections": corrections
    }))
}

fn behavior_versions(row: &sqlx::postgres::PgRow) -> Result<Value, sqlx::Error> {
    Ok(json!({
        "application_version": row.try_get::<String, _>("application_version")?,
        "parser_schema_version": row.try_get::<String, _>("parser_schema_version")?,
        "prompt_version": row.try_get::<String, _>("prompt_version")?,
        "model_provider_version": row.try_get::<String, _>("model_provider_version")?,
        "normalization_version": row.try_get::<String, _>("normalization_version")?,
        "resolution_policy_version": row.try_get::<String, _>("resolution_policy_version")?,
        "portion_policy_version": row.try_get::<String, _>("portion_policy_version")?,
        "composition_policy_version": row.try_get::<String, _>("composition_policy_version")?,
        "clarification_policy_version": row.try_get::<String, _>("clarification_policy_version")?,
        "correction_policy_version": row.try_get::<String, _>("correction_policy_version")?,
        "calculation_engine_version": row.try_get::<String, _>("calculation_engine_version")?,
        "catalog_release_id": row.try_get::<String, _>("catalog_release_id")?
    }))
}

async fn export_clarifications(pool: &PgPool, user_id: UserId) -> Result<Vec<Value>, PrivacyError> {
    let rows = sqlx::query(
        "SELECT question.id::text AS question_id,
                meal.id::text AS analysis_id,
                question.dimension,
                question.options,
                question.policy_version,
                question.status,
                question.created_at::text AS created_at,
                question.answered_at::text AS answered_at
           FROM analysis.clarification_question question
           JOIN analysis.analysis_revision revision
             ON revision.id = question.analysis_revision_id
           JOIN analysis.meal_analysis meal
             ON meal.id = revision.meal_analysis_id
          WHERE meal.user_id = $1
          ORDER BY question.created_at",
    )
    .bind(user_id.as_uuid())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(json!({
                "question_id": row.try_get::<String, _>("question_id")?,
                "analysis_id": row.try_get::<String, _>("analysis_id")?,
                "dimension": row.try_get::<String, _>("dimension")?,
                "options": row.try_get::<Value, _>("options")?,
                "policy_version": row.try_get::<String, _>("policy_version")?,
                "status": row.try_get::<String, _>("status")?,
                "created_at": row.try_get::<String, _>("created_at")?,
                "answered_at": row.try_get::<Option<String>, _>("answered_at")?,
                "prompt": REDACTED
            }))
        })
        .collect()
}

async fn export_corrections(pool: &PgPool, user_id: UserId) -> Result<Vec<Value>, PrivacyError> {
    let rows = sqlx::query(
        "SELECT correction.id::text AS correction_id,
                correction.meal_analysis_id::text AS analysis_id,
                correction.base_revision_id::text AS base_revision_id,
                correction.created_revision_id::text AS created_revision_id,
                correction.correction_payload,
                correction.created_at::text AS created_at
           FROM app.analysis_correction correction
           JOIN analysis.meal_analysis meal
             ON meal.id = correction.meal_analysis_id
          WHERE meal.user_id = $1
          ORDER BY correction.created_at",
    )
    .bind(user_id.as_uuid())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let mut payload = row.try_get::<Value, _>("correction_payload")?;
            redact_export_value(&mut payload);
            Ok(json!({
                "correction_id": row.try_get::<String, _>("correction_id")?,
                "analysis_id": row.try_get::<String, _>("analysis_id")?,
                "base_revision_id": row.try_get::<String, _>("base_revision_id")?,
                "created_revision_id": row.try_get::<Option<String>, _>("created_revision_id")?,
                "correction": payload,
                "created_at": row.try_get::<String, _>("created_at")?
            }))
        })
        .collect()
}

/// Purges all user-owned analysis aggregates and then removes the external identity mapping.
///
/// # Errors
///
/// Returns [`PrivacyError`] when the request reference is invalid or the transactional purge
/// cannot complete.
pub async fn delete_user_data(
    pool: &PgPool,
    user_id: UserId,
    request_reference: &str,
) -> Result<PrivacyDeletionReceipt, PrivacyError> {
    validate_request_reference(request_reference)?;
    let mut transaction = pool.begin().await?;
    enable_privacy_purge(&mut transaction).await?;
    purge_user_data_tx(&mut transaction, user_id).await?;
    sqlx::query("DELETE FROM auth.external_identity WHERE user_id = $1")
        .bind(user_id.as_uuid())
        .execute(&mut *transaction)
        .await?;
    let deleted_at: String = sqlx::query_scalar(
        "INSERT INTO ops.audit_event (id, action, target_type, target_id, metadata)
         VALUES ($1, 'privacy.deletion_completed', 'user', $2, $3)
         RETURNING created_at::text",
    )
    .bind(Uuid::now_v7())
    .bind(user_id.as_uuid())
    .bind(json!({"request_reference": request_reference}))
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(PrivacyDeletionReceipt {
        event_type: "privacy.deletion_completed",
        deleted_at,
        request_reference: request_reference.to_owned(),
    })
}

/// Runs the bounded 30/365-day retention policy in one transaction.
///
/// # Errors
///
/// Returns [`PrivacyError`] when any retention query fails; the transaction is then rolled back.
pub async fn run_privacy_retention(pool: &PgPool) -> Result<PrivacyRetentionReport, PrivacyError> {
    let mut transaction = pool.begin().await?;
    enable_privacy_purge(&mut transaction).await?;
    let deleted_parser_telemetry = sqlx::query(
        "DELETE FROM ops.parser_invocation
          WHERE created_at < now() - interval '30 days'",
    )
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    let deleted_audit_events = sqlx::query(
        "DELETE FROM ops.audit_event
          WHERE created_at < now() - interval '365 days'",
    )
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    let expired_users = sqlx::query_scalar::<_, Uuid>(
        "SELECT meal.user_id
           FROM analysis.meal_analysis meal
           LEFT JOIN analysis.analysis_revision revision
             ON revision.meal_analysis_id = meal.id
          WHERE meal.user_id IS NOT NULL
          GROUP BY meal.user_id
         HAVING COALESCE(MAX(revision.created_at), MAX(meal.created_at))
                < now() - interval '365 days'
          ORDER BY meal.user_id",
    )
    .fetch_all(&mut *transaction)
    .await?;
    let mut purged_user_aggregates = 0;
    for user_id in expired_users {
        if purge_user_data_tx(&mut transaction, UserId::from_uuid(user_id)).await? {
            sqlx::query("DELETE FROM auth.external_identity WHERE user_id = $1")
                .bind(user_id)
                .execute(&mut *transaction)
                .await?;
            purged_user_aggregates += 1;
        }
    }
    transaction.commit().await?;
    Ok(PrivacyRetentionReport {
        deleted_parser_telemetry,
        deleted_audit_events,
        purged_user_aggregates,
    })
}

async fn enable_privacy_purge(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('app.privacy_purge', 'true', true)")
        .execute(&mut **transaction)
        .await
        .map(|_| ())
}

async fn purge_user_data_tx(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: UserId,
) -> Result<bool, sqlx::Error> {
    let analysis_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM analysis.meal_analysis WHERE user_id = $1 FOR UPDATE",
    )
    .bind(user_id.as_uuid())
    .fetch_all(&mut **transaction)
    .await?;
    if analysis_ids.is_empty() {
        return Ok(false);
    }
    let analysis_id_text = analysis_ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    for statement in [
        "DELETE FROM ops.outbox_event WHERE aggregate_id = ANY($1::uuid[])",
        "DELETE FROM ops.audit_event WHERE target_type = 'analysis' AND target_id = ANY($1::uuid[])",
        "DELETE FROM ops.job WHERE payload->>'analysis_id' = ANY($2::text[])",
        "DELETE FROM analysis.clarification_answer
          WHERE question_id IN (
              SELECT question.id FROM analysis.clarification_question question
              JOIN analysis.analysis_revision revision ON revision.id = question.analysis_revision_id
              WHERE revision.meal_analysis_id = ANY($1::uuid[])
          )",
        "DELETE FROM analysis.clarification_question
          WHERE analysis_revision_id IN (
              SELECT id FROM analysis.analysis_revision WHERE meal_analysis_id = ANY($1::uuid[])
          )",
        "DELETE FROM app.analysis_correction WHERE meal_analysis_id = ANY($1::uuid[])",
        "DELETE FROM analysis.item_nutrient_result
          WHERE item_id IN (
              SELECT item.id FROM analysis.analysis_item item
              JOIN analysis.analysis_revision revision ON revision.id = item.revision_id
              WHERE revision.meal_analysis_id = ANY($1::uuid[])
          )",
        "DELETE FROM analysis.resolution_candidate
          WHERE item_id IN (
              SELECT item.id FROM analysis.analysis_item item
              JOIN analysis.analysis_revision revision ON revision.id = item.revision_id
              WHERE revision.meal_analysis_id = ANY($1::uuid[])
          )",
        "DELETE FROM analysis.revision_nutrient_total
          WHERE revision_id IN (
              SELECT id FROM analysis.analysis_revision WHERE meal_analysis_id = ANY($1::uuid[])
          )",
        "UPDATE analysis.meal_analysis
            SET current_revision_id = NULL
          WHERE id = ANY($1::uuid[])",
        "DELETE FROM analysis.analysis_item
          WHERE revision_id IN (
              SELECT id FROM analysis.analysis_revision WHERE meal_analysis_id = ANY($1::uuid[])
          )",
        "DELETE FROM analysis.analysis_revision WHERE meal_analysis_id = ANY($1::uuid[])",
        "DELETE FROM analysis.meal_analysis WHERE id = ANY($1::uuid[])",
        "DELETE FROM app.idempotency_record WHERE scope_key LIKE $1",
    ] {
        if statement.contains("ops.job") {
            sqlx::query(statement)
                .bind(&analysis_ids)
                .bind(&analysis_id_text)
                .execute(&mut **transaction)
                .await?;
        } else if statement.contains("idempotency_record") {
            sqlx::query(statement)
                .bind(format!("user:{}:%", user_id.as_uuid()))
                .execute(&mut **transaction)
                .await?;
        } else {
            sqlx::query(statement)
                .bind(&analysis_ids)
                .execute(&mut **transaction)
                .await?;
        }
    }
    Ok(true)
}

fn validate_request_reference(value: &str) -> Result<(), PrivacyError> {
    if value.trim().is_empty() || value.len() > 128 || !value.is_ascii() {
        return Err(PrivacyError::InvalidInput(
            "request_reference must be non-empty ASCII with at most 128 characters".to_owned(),
        ));
    }
    Ok(())
}
