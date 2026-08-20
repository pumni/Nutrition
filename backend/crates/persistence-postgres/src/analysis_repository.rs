use crate::privacy::redact_persisted_value;
use application::{
    AnalysisListEntry, AnalysisListQuery, AnalysisRepository, AnalysisSnapshot,
    AnalysisSnapshotReader, AnalysisWorkflow, ApplicationError, ClarificationAnalysis,
    ClarificationAnswerRequest, CorrectionRequest, WorkflowQuestion,
};
use async_trait::async_trait;
use domain::{
    AnalysisId, AnalysisItemId, AnalysisRevisionId, EvidenceQuality, MassResolutionMethod,
    NutrientCode, NutrientUnit, UserId, ValueStatus,
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

    /// Claims an idempotency key before the application operation starts.
    ///
    /// A short-lived empty response reference represents an in-flight request. A concurrent
    /// request with the same typed hash waits for that reference to be finalized and then
    /// replays the immutable response. A completed record is retained for 24 hours.
    ///
    /// # Errors
    ///
    /// Returns `IdempotencyConflict` when the key is active with another request hash.
    pub async fn reserve_idempotency(
        &self,
        scope_key: &str,
        key: &str,
        request_hash: &str,
    ) -> Result<Option<Value>, ApplicationError> {
        for _ in 0..50 {
            let claimed = sqlx::query(
                r"
                INSERT INTO app.idempotency_record (
                    scope_key, idempotency_key, request_hash,
                    response_reference, expires_at
                ) VALUES ($1, $2, $3, NULL, now() + interval '60 seconds')
                ON CONFLICT (scope_key, idempotency_key) DO UPDATE
                    SET request_hash = EXCLUDED.request_hash,
                        response_reference = NULL,
                        expires_at = EXCLUDED.expires_at,
                        created_at = EXCLUDED.created_at
                  WHERE app.idempotency_record.expires_at <= now()
                RETURNING scope_key
                ",
            )
            .bind(scope_key)
            .bind(key)
            .bind(request_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ApplicationError::Persistence)?;
            if claimed.is_some() {
                return Ok(None);
            }

            let existing_hash = sqlx::query_scalar::<_, String>(
                "SELECT request_hash
                   FROM app.idempotency_record
                  WHERE scope_key = $1
                    AND idempotency_key = $2
                    AND expires_at > now()",
            )
            .bind(scope_key)
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ApplicationError::Persistence)?;
            let Some(existing_hash) = existing_hash else {
                continue;
            };
            if existing_hash != request_hash {
                return Err(ApplicationError::IdempotencyConflict);
            }
            if let Some(response) = self
                .find_idempotent_response(scope_key, key, request_hash)
                .await?
            {
                return Ok(Some(response));
            }

            sqlx::query("SELECT pg_sleep(0.1)")
                .execute(&self.pool)
                .await
                .map_err(|_| ApplicationError::Persistence)?;
        }
        Err(ApplicationError::Persistence)
    }

    /// Returns the prior immutable response for a matching idempotency key.
    ///
    /// # Errors
    ///
    /// Returns `IdempotencyConflict` when the key exists with another request hash.
    pub async fn find_idempotent_response(
        &self,
        scope_key: &str,
        key: &str,
        request_hash: &str,
    ) -> Result<Option<Value>, ApplicationError> {
        let row = sqlx::query(
            r"
            SELECT record.request_hash,
                   revision.result_snapshot,
                   revision.snapshot_hash
            FROM app.idempotency_record record
            JOIN analysis.analysis_revision revision
              ON revision.id = (record.response_reference->>'revision_id')::uuid
            WHERE record.scope_key = $1
              AND record.idempotency_key = $2
              AND record.expires_at > now()
            ",
        )
        .bind(scope_key)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ApplicationError::Persistence)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let existing_hash: String = row
            .try_get("request_hash")
            .map_err(|_| ApplicationError::Persistence)?;
        if existing_hash != request_hash {
            return Err(ApplicationError::IdempotencyConflict);
        }
        verified_snapshot_value(&row).map(Some)
    }

    /// Checks whether an authenticated user owns an analysis.
    ///
    /// # Errors
    ///
    /// Returns `Persistence` when `PostgreSQL` cannot perform the ownership query.
    pub async fn authorize_analysis(
        &self,
        analysis_id: AnalysisId,
        user_id: UserId,
    ) -> Result<bool, ApplicationError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM analysis.meal_analysis
                WHERE id = $1 AND user_id = $2
            )",
        )
        .bind(analysis_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ApplicationError::Persistence)
    }

    /// Resolves an OIDC `(issuer, subject)` pair to a stable internal user identity.
    ///
    /// The `UUIDv7` is generated inside the transaction that creates the first mapping. A
    /// concurrent first login for the same external identity converges on the existing row via
    /// the primary-key conflict path.
    ///
    /// # Errors
    ///
    /// Returns `Persistence` when the mapping cannot be read or created.
    pub async fn resolve_external_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<UserId, ApplicationError> {
        if issuer.trim().is_empty() || subject.trim().is_empty() {
            return Err(ApplicationError::Unauthorized);
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| ApplicationError::Persistence)?;
        let user_id = Uuid::now_v7();
        let resolved_user_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO auth.external_identity (issuer, subject, user_id)
             VALUES ($1, $2, $3)
             ON CONFLICT (issuer, subject) DO UPDATE
                 SET issuer = EXCLUDED.issuer
             RETURNING user_id",
        )
        .bind(issuer)
        .bind(subject)
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| ApplicationError::Persistence)?;
        transaction
            .commit()
            .await
            .map_err(|_| ApplicationError::Persistence)?;
        Ok(UserId::from_uuid(resolved_user_id))
    }
}

#[async_trait]
impl AnalysisRepository for PostgresAnalysisRepository {
    async fn save(&self, snapshot: &AnalysisSnapshot) -> Result<(), ApplicationError> {
        persist_snapshot(&self.pool, snapshot).await
    }

    async fn save_clarification(
        &self,
        clarification: &ClarificationAnalysis,
    ) -> Result<(), ApplicationError> {
        persist_clarification(&self.pool, clarification).await
    }

    async fn find_open_clarification(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<ClarificationAnalysis>, ApplicationError> {
        find_open_clarification(&self.pool, analysis_id).await
    }

    async fn append_clarification_answer(
        &self,
        answer: &ClarificationAnswerRequest,
        snapshot: &AnalysisSnapshot,
    ) -> Result<(), ApplicationError> {
        persist_clarification_answer(&self.pool, answer, snapshot).await
    }

    async fn append_correction(
        &self,
        request: &CorrectionRequest,
        snapshot: &AnalysisSnapshot,
    ) -> Result<(), ApplicationError> {
        persist_correction(&self.pool, request, snapshot).await
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

        row.map(|snapshot_row| decode_snapshot_row(&snapshot_row))
            .transpose()
    }

    async fn find_revision(
        &self,
        analysis_id: AnalysisId,
        revision_number: u32,
    ) -> Result<Option<Value>, ApplicationError> {
        let row = sqlx::query(
            r"
            SELECT revision.result_snapshot, revision.snapshot_hash
            FROM analysis.analysis_revision revision
            WHERE revision.meal_analysis_id = $1
              AND revision.revision_number = $2
            ",
        )
        .bind(analysis_id.as_uuid())
        .bind(i32::try_from(revision_number).map_err(|_| {
            ApplicationError::InvalidInput("revision number is too large".to_owned())
        })?)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ApplicationError::Persistence)?;
        row.map(|snapshot_row| verified_snapshot_value(&snapshot_row))
            .transpose()
    }

    async fn current_revision_id(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisRevisionId>, ApplicationError> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT current_revision_id FROM analysis.meal_analysis WHERE id = $1",
        )
        .bind(analysis_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map(|value| value.map(AnalysisRevisionId::from_uuid))
        .map_err(|_| ApplicationError::Persistence)
    }

    async fn list(
        &self,
        user_id: UserId,
        query: AnalysisListQuery,
    ) -> Result<Vec<AnalysisListEntry>, ApplicationError> {
        let rows = sqlx::query(
            r#"
            SELECT meal.id,
                   meal.status,
                   meal.locale,
                   to_char(meal.created_at AT TIME ZONE 'UTC',
                           'YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at,
                   revision.revision_number,
                   revision.result_status,
                   revision.quality_label
            FROM analysis.meal_analysis meal
            LEFT JOIN analysis.analysis_revision revision
              ON revision.id = meal.current_revision_id
            WHERE meal.user_id = $1
              AND ($2::text IS NULL OR meal.status = $2)
              AND ($3::text IS NULL OR meal.locale = $3)
              AND meal.created_at <= to_timestamp($4::double precision)
              AND (
                    $5::text IS NULL
                    OR meal.created_at < $5::timestamptz
                    OR (meal.created_at = $5::timestamptz AND meal.id < $6)
              )
            ORDER BY meal.created_at DESC, meal.id DESC
            LIMIT $7
            "#,
        )
        .bind(user_id.as_uuid())
        .bind(query.status.as_deref())
        .bind(query.locale.as_deref())
        .bind(query.snapshot_epoch_seconds)
        .bind(query.after_created_at.as_deref())
        .bind(query.after_analysis_id.map(AnalysisId::as_uuid))
        .bind(query.limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ApplicationError::Persistence)?;

        rows.into_iter()
            .map(|row| {
                let revision_number: Option<i32> = row
                    .try_get("revision_number")
                    .map_err(|_| ApplicationError::Persistence)?;
                Ok(AnalysisListEntry {
                    analysis_id: AnalysisId::from_uuid(
                        row.try_get("id")
                            .map_err(|_| ApplicationError::Persistence)?,
                    ),
                    status: row
                        .try_get("status")
                        .map_err(|_| ApplicationError::Persistence)?,
                    locale: row
                        .try_get("locale")
                        .map_err(|_| ApplicationError::Persistence)?,
                    created_at: row
                        .try_get("created_at")
                        .map_err(|_| ApplicationError::Persistence)?,
                    current_revision_number: revision_number
                        .map(|value| {
                            u32::try_from(value).map_err(|_| ApplicationError::Persistence)
                        })
                        .transpose()?,
                    result_status: row
                        .try_get("result_status")
                        .map_err(|_| ApplicationError::Persistence)?,
                    quality_label: row
                        .try_get("quality_label")
                        .map_err(|_| ApplicationError::Persistence)?,
                })
            })
            .collect()
    }

    async fn workflow(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisWorkflow>, ApplicationError> {
        let row = sqlx::query(
            r"
            SELECT meal.id,
                   meal.status,
                   revision.id AS revision_id,
                   revision.revision_number,
                   question.id AS question_id,
                   question.dimension,
                   question.prompt,
                   question.options
            FROM analysis.meal_analysis meal
            LEFT JOIN analysis.analysis_revision revision
              ON revision.id = meal.current_revision_id
            LEFT JOIN analysis.clarification_question question
              ON question.analysis_revision_id = revision.id
             AND question.status = 'open'
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

        let status: String = row
            .try_get("status")
            .map_err(|_| ApplicationError::Persistence)?;
        let revision_number: Option<i32> = row
            .try_get("revision_number")
            .map_err(|_| ApplicationError::Persistence)?;
        let question_id: Option<Uuid> = row
            .try_get("question_id")
            .map_err(|_| ApplicationError::Persistence)?;
        let pending_question = match question_id {
            Some(question_id) => Some(WorkflowQuestion {
                question_id: domain::ClarificationQuestionId::from_uuid(question_id),
                dimension: row
                    .try_get("dimension")
                    .map_err(|_| ApplicationError::Persistence)?,
                prompt: row
                    .try_get("prompt")
                    .map_err(|_| ApplicationError::Persistence)?,
                options: serde_json::from_value(
                    row.try_get("options")
                        .map_err(|_| ApplicationError::Persistence)?,
                )
                .map_err(|_| ApplicationError::Persistence)?,
            }),
            None => None,
        };
        let current_revision_number = revision_number
            .map(|value| u32::try_from(value).map_err(|_| ApplicationError::Persistence))
            .transpose()?;

        Ok(Some(AnalysisWorkflow {
            analysis_id,
            current_revision_id: row
                .try_get::<Option<Uuid>, _>("revision_id")
                .map_err(|_| ApplicationError::Persistence)?
                .map(AnalysisRevisionId::from_uuid),
            current_revision_number,
            state: status.clone(),
            pending_question,
            allowed_actions: match status.as_str() {
                "needs_clarification" => vec!["answer_clarification".to_owned()],
                "completed" | "confirmed" | "corrected" | "insufficient_evidence" => {
                    vec!["correct".to_owned()]
                }
                _ => Vec::new(),
            },
        }))
    }
}

async fn persist_snapshot(
    pool: &PgPool,
    snapshot: &AnalysisSnapshot,
) -> Result<(), ApplicationError> {
    let snapshot_value =
        serde_json::to_value(snapshot).map_err(|_| ApplicationError::Persistence)?;
    let mut snapshot_value = snapshot_value;
    redact_persisted_value(&mut snapshot_value);
    let snapshot_bytes =
        serde_json::to_vec(&snapshot_value).map_err(|_| ApplicationError::Persistence)?;
    let snapshot_hash = sha256_hex(&snapshot_bytes);
    let nutrient_ids = load_nutrient_ids(pool, &snapshot.requested_nutrients).await?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| ApplicationError::Persistence)?;

    insert_analysis(&mut transaction, snapshot).await?;
    insert_revision(&mut transaction, snapshot, "initial_analysis").await?;
    insert_items_and_results(&mut transaction, snapshot, &nutrient_ids).await?;
    insert_totals(&mut transaction, snapshot, &nutrient_ids).await?;
    finalize_revision(&mut transaction, snapshot, snapshot_value, &snapshot_hash).await?;
    finalize_analysis(&mut transaction, snapshot).await?;
    insert_outbox(&mut transaction, snapshot).await?;
    insert_idempotency(
        &mut transaction,
        snapshot.idempotency.as_ref(),
        snapshot.analysis_id,
        snapshot.revision_id,
    )
    .await?;

    transaction
        .commit()
        .await
        .map_err(|_| ApplicationError::Persistence)
}

async fn persist_clarification(
    pool: &PgPool,
    clarification: &ClarificationAnalysis,
) -> Result<(), ApplicationError> {
    let snapshot_value =
        serde_json::to_value(clarification).map_err(|_| ApplicationError::Persistence)?;
    let mut snapshot_value = snapshot_value;
    redact_persisted_value(&mut snapshot_value);
    let snapshot_hash = snapshot_hash(&snapshot_value)?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| ApplicationError::Persistence)?;
    sqlx::query(
        "INSERT INTO analysis.meal_analysis (id, user_id, locale, idempotency_key, status)
         VALUES ($1, $2, $3, $4, 'resolving')",
    )
    .bind(clarification.analysis_id.as_uuid())
    .bind(clarification.owner_id.map(UserId::as_uuid))
    .bind(&clarification.locale)
    .bind(
        clarification
            .idempotency
            .as_ref()
            .map(|context| context.key.as_str()),
    )
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    insert_clarification_revision(&mut transaction, clarification).await?;
    sqlx::query(
        r"
        INSERT INTO analysis.clarification_question (
            id, analysis_revision_id, dimension, prompt, options,
            policy_version, status, context_payload
        ) VALUES ($1, $2, $3, $4, $5, $6, 'open', $7)
        ",
    )
    .bind(clarification.question.id.as_uuid())
    .bind(clarification.revision_id.as_uuid())
    .bind(&clarification.question.dimension)
    .bind(&clarification.question.prompt)
    .bind(
        serde_json::to_value(&clarification.question.options)
            .map_err(|_| ApplicationError::Persistence)?,
    )
    .bind("clarification-portion-0.1.0")
    .bind(safe_clarification_context(&clarification.context)?)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    finalize_revision_value(
        &mut transaction,
        clarification.revision_id,
        snapshot_value,
        &snapshot_hash,
    )
    .await?;
    let changed = sqlx::query(
        r"
        UPDATE analysis.meal_analysis
           SET status = 'needs_clarification',
               current_revision_id = $2
         WHERE id = $1
           AND status = 'resolving'
        ",
    )
    .bind(clarification.analysis_id.as_uuid())
    .bind(clarification.revision_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    if changed.rows_affected() != 1 {
        return Err(ApplicationError::Persistence);
    }
    insert_workflow_outbox(
        &mut transaction,
        clarification.analysis_id,
        clarification.revision_id,
        "AnalysisNeedsClarification",
    )
    .await?;
    insert_idempotency(
        &mut transaction,
        clarification.idempotency.as_ref(),
        clarification.analysis_id,
        clarification.revision_id,
    )
    .await?;
    transaction
        .commit()
        .await
        .map_err(|_| ApplicationError::Persistence)
}

async fn find_open_clarification(
    pool: &PgPool,
    analysis_id: AnalysisId,
) -> Result<Option<ClarificationAnalysis>, ApplicationError> {
    let row = sqlx::query(
        r"
        SELECT revision.result_snapshot,
               revision.snapshot_hash,
               question.context_payload
        FROM analysis.meal_analysis meal
        JOIN analysis.analysis_revision revision
          ON revision.id = meal.current_revision_id
        JOIN analysis.clarification_question question
          ON question.analysis_revision_id = revision.id
         AND question.status = 'open'
        WHERE meal.id = $1
          AND meal.status = 'needs_clarification'
        ",
    )
    .bind(analysis_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let mut value = verified_snapshot_value(&row)?;
    let context: Value = row
        .try_get("context_payload")
        .map_err(|_| ApplicationError::Persistence)?;
    value
        .as_object_mut()
        .ok_or(ApplicationError::Persistence)?
        .insert("context".to_owned(), context);
    serde_json::from_value(value)
        .map(Some)
        .map_err(|_| ApplicationError::Persistence)
}

async fn persist_clarification_answer(
    pool: &PgPool,
    answer: &ClarificationAnswerRequest,
    snapshot: &AnalysisSnapshot,
) -> Result<(), ApplicationError> {
    let nutrient_ids = load_nutrient_ids(pool, &snapshot.requested_nutrients).await?;
    let snapshot_value =
        serde_json::to_value(snapshot).map_err(|_| ApplicationError::Persistence)?;
    let mut snapshot_value = snapshot_value;
    redact_persisted_value(&mut snapshot_value);
    let snapshot_hash = snapshot_hash(&snapshot_value)?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| ApplicationError::Persistence)?;
    lock_current_revision(
        &mut transaction,
        snapshot.analysis_id,
        answer.expected_revision_id,
        "needs_clarification",
        ApplicationError::StaleClarification,
    )
    .await?;
    let question_revision: Option<Uuid> = sqlx::query_scalar(
        r"
        SELECT analysis_revision_id
        FROM analysis.clarification_question
        WHERE id = $1
          AND status = 'open'
        FOR UPDATE
        ",
    )
    .bind(answer.question_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    if question_revision != Some(answer.expected_revision_id.as_uuid()) {
        return Err(ApplicationError::StaleClarification);
    }
    transition_status(
        &mut transaction,
        snapshot.analysis_id,
        "needs_clarification",
        "resolving",
    )
    .await?;
    insert_revision(&mut transaction, snapshot, "clarification_answer_applied").await?;
    insert_items_and_results(&mut transaction, snapshot, &nutrient_ids).await?;
    insert_totals(&mut transaction, snapshot, &nutrient_ids).await?;
    finalize_revision(&mut transaction, snapshot, snapshot_value, &snapshot_hash).await?;
    sqlx::query(
        r"
        INSERT INTO analysis.clarification_answer (
            id, question_id, expected_revision_id, option_id, created_revision_id
        ) VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(Uuid::now_v7())
    .bind(answer.question_id.as_uuid())
    .bind(answer.expected_revision_id.as_uuid())
    .bind(&answer.option_id)
    .bind(snapshot.revision_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    sqlx::query(
        "UPDATE analysis.clarification_question
            SET status = 'answered', answered_at = now()
          WHERE id = $1 AND status = 'open'",
    )
    .bind(answer.question_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    set_completed_from(&mut transaction, snapshot, "resolving").await?;
    insert_outbox(&mut transaction, snapshot).await?;
    insert_idempotency(
        &mut transaction,
        answer.idempotency.as_ref(),
        snapshot.analysis_id,
        snapshot.revision_id,
    )
    .await?;
    transaction
        .commit()
        .await
        .map_err(|_| ApplicationError::Persistence)
}

async fn persist_correction(
    pool: &PgPool,
    request: &CorrectionRequest,
    snapshot: &AnalysisSnapshot,
) -> Result<(), ApplicationError> {
    let nutrient_ids = load_nutrient_ids(pool, &snapshot.requested_nutrients).await?;
    let snapshot_value =
        serde_json::to_value(snapshot).map_err(|_| ApplicationError::Persistence)?;
    let mut snapshot_value = snapshot_value;
    redact_persisted_value(&mut snapshot_value);
    let snapshot_hash = snapshot_hash(&snapshot_value)?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| ApplicationError::Persistence)?;
    lock_current_revision(
        &mut transaction,
        snapshot.analysis_id,
        request.base_revision_id,
        "completed",
        ApplicationError::RevisionConflict,
    )
    .await?;
    transition_status(
        &mut transaction,
        snapshot.analysis_id,
        "completed",
        "corrected",
    )
    .await?;
    insert_revision(&mut transaction, snapshot, "user_correction").await?;
    insert_items_and_results(&mut transaction, snapshot, &nutrient_ids).await?;
    insert_totals(&mut transaction, snapshot, &nutrient_ids).await?;
    finalize_revision(&mut transaction, snapshot, snapshot_value, &snapshot_hash).await?;
    sqlx::query(
        r"
        INSERT INTO app.analysis_correction (
            id, meal_analysis_id, base_revision_id, actor_type,
            correction_payload, created_revision_id
        ) VALUES ($1, $2, $3, 'user', $4, $5)
        ",
    )
    .bind(Uuid::now_v7())
    .bind(snapshot.analysis_id.as_uuid())
    .bind(request.base_revision_id.as_uuid())
    .bind(serde_json::to_value(request).map_err(|_| ApplicationError::Persistence)?)
    .bind(snapshot.revision_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    set_completed_from(&mut transaction, snapshot, "corrected").await?;
    insert_outbox(&mut transaction, snapshot).await?;
    insert_idempotency(
        &mut transaction,
        request.idempotency.as_ref(),
        snapshot.analysis_id,
        snapshot.revision_id,
    )
    .await?;
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

async fn insert_revision(
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

async fn insert_clarification_revision(
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

async fn lock_current_revision(
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

async fn transition_status(
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

async fn set_completed_from(
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

async fn finalize_revision_value(
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

async fn insert_workflow_outbox(
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

async fn insert_idempotency(
    transaction: &mut Transaction<'_, Postgres>,
    context: Option<&application::IdempotencyContext>,
    analysis_id: AnalysisId,
    revision_id: AnalysisRevisionId,
) -> Result<(), ApplicationError> {
    let Some(context) = context else {
        return Ok(());
    };
    sqlx::query(
        r"
        UPDATE app.idempotency_record
           SET response_reference = $4,
               expires_at = now() + interval '24 hours'
         WHERE scope_key = $1
           AND idempotency_key = $2
           AND request_hash = $3
           AND expires_at > now()
        ",
    )
    .bind(&context.scope_key)
    .bind(&context.key)
    .bind(&context.request_hash)
    .bind(json!({
        "analysis_id": analysis_id,
        "revision_id": revision_id
    }))
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)
    .and_then(|result| {
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(ApplicationError::Persistence)
        }
    })?;
    Ok(())
}

fn snapshot_hash(snapshot_value: &Value) -> Result<String, ApplicationError> {
    let bytes = serde_json::to_vec(snapshot_value).map_err(|_| ApplicationError::Persistence)?;
    Ok(sha256_hex(&bytes))
}

fn verified_snapshot_value(row: &sqlx::postgres::PgRow) -> Result<Value, ApplicationError> {
    let value: Value = row
        .try_get("result_snapshot")
        .map_err(|_| ApplicationError::Persistence)?;
    let expected_hash: String = row
        .try_get("snapshot_hash")
        .map_err(|_| ApplicationError::Persistence)?;
    if snapshot_hash(&value)? != expected_hash {
        return Err(ApplicationError::Persistence);
    }
    Ok(value)
}

fn decode_snapshot_row(row: &sqlx::postgres::PgRow) -> Result<AnalysisSnapshot, ApplicationError> {
    serde_json::from_value(verified_snapshot_value(row)?).map_err(|_| ApplicationError::Persistence)
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

fn safe_clarification_context(
    context: &application::ClarificationContext,
) -> Result<Value, ApplicationError> {
    let mut value = serde_json::to_value(context).map_err(|_| ApplicationError::Persistence)?;
    redact_persisted_value(&mut value);
    Ok(value)
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

const fn resolution_status_code(value: MassResolutionMethod) -> &'static str {
    match value {
        MassResolutionMethod::ExplicitMass => "resolved_exact",
        MassResolutionMethod::VolumeDensity
        | MassResolutionMethod::BrandedServing
        | MassResolutionMethod::PortionObservation
        | MassResolutionMethod::CuratedDefault => "resolved_with_assumption",
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
