//! Idempotency key, request hash, replay, and conflict responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

impl PostgresAnalysisRepository {
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
        crate::telemetry::observe_db_future(
            "idempotency_reserve",
            self.reserve_idempotency_inner(scope_key, key, request_hash),
        )
        .await
    }

    async fn reserve_idempotency_inner(
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
        crate::telemetry::observe_db_future(
            "idempotency_replay_lookup",
            self.find_idempotent_response_inner(scope_key, key, request_hash),
        )
        .await
    }

    async fn find_idempotent_response_inner(
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
}
pub(crate) async fn insert_idempotency(
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
