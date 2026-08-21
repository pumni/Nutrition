//! Current and historical analysis read responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[async_trait]
impl AnalysisSnapshotReader for PostgresAnalysisRepository {
    async fn find(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisSnapshot>, ApplicationError> {
        crate::telemetry::observe_db_future("analysis_find", async {
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
        })
        .await
    }

    async fn find_revision(
        &self,
        analysis_id: AnalysisId,
        revision_number: u32,
    ) -> Result<Option<Value>, ApplicationError> {
        crate::telemetry::observe_db_future("analysis_find_revision", async {
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
        })
        .await
    }

    async fn current_revision_id(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisRevisionId>, ApplicationError> {
        let revision_id = crate::telemetry::observe_db_future("analysis_current_revision", async {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT current_revision_id FROM analysis.meal_analysis WHERE id = $1",
            )
            .bind(analysis_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ApplicationError::Persistence)
        })
        .await?;
        Ok(revision_id.map(AnalysisRevisionId::from_uuid))
    }

    async fn list(
        &self,
        user_id: UserId,
        query: AnalysisListQuery,
    ) -> Result<Vec<AnalysisListEntry>, ApplicationError> {
        crate::telemetry::observe_db_future("analysis_list", async {
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
        })
        .await
    }

    async fn workflow(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisWorkflow>, ApplicationError> {
        let started = std::time::Instant::now();
        let row_result = sqlx::query(
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
        .map_err(|_| ApplicationError::Persistence);
        crate::telemetry::record_db_operation(
            "analysis_workflow",
            started,
            if row_result.is_ok() {
                "success"
            } else {
                "failure"
            },
        );
        let row = row_result?;
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

pub(crate) async fn find_open_clarification(
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
