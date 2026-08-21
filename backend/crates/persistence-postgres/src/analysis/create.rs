//! Analysis and revision creation transaction responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[async_trait]
impl AnalysisRepository for PostgresAnalysisRepository {
    async fn save(&self, snapshot: &AnalysisSnapshot) -> Result<(), ApplicationError> {
        crate::telemetry::observe_db_future("analysis_save", persist_snapshot(&self.pool, snapshot))
            .await
    }

    async fn save_clarification(
        &self,
        clarification: &ClarificationAnalysis,
    ) -> Result<(), ApplicationError> {
        crate::telemetry::observe_db_future(
            "analysis_save_clarification",
            persist_clarification(&self.pool, clarification),
        )
        .await
    }

    async fn find_open_clarification(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<ClarificationAnalysis>, ApplicationError> {
        crate::telemetry::observe_db_future(
            "analysis_find_open_clarification",
            find_open_clarification(&self.pool, analysis_id),
        )
        .await
    }

    async fn append_clarification_answer(
        &self,
        answer: &ClarificationAnswerRequest,
        snapshot: &AnalysisSnapshot,
    ) -> Result<(), ApplicationError> {
        crate::telemetry::observe_db_future(
            "analysis_append_clarification_answer",
            persist_clarification_answer(&self.pool, answer, snapshot),
        )
        .await
    }

    async fn append_correction(
        &self,
        request: &CorrectionRequest,
        snapshot: &AnalysisSnapshot,
    ) -> Result<(), ApplicationError> {
        crate::telemetry::observe_db_future(
            "analysis_append_correction",
            persist_correction(&self.pool, request, snapshot),
        )
        .await
    }
}

pub(crate) async fn persist_snapshot(
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

pub(crate) async fn persist_clarification(
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

pub(crate) async fn persist_clarification_answer(
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

pub(crate) async fn persist_correction(
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
