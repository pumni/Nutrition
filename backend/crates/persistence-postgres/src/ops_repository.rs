use application::ApplicationError;
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub job_type: String,
    pub payload: Value,
    pub attempts: i32,
    pub max_attempts: i32,
}

/// Claims an available bounded job batch using `SKIP LOCKED`.
///
/// # Errors
///
/// Returns `Persistence` when `PostgreSQL` cannot atomically claim the batch.
pub async fn claim_jobs(
    pool: &PgPool,
    worker_id: &str,
    limit: i64,
) -> Result<Vec<ClaimedJob>, ApplicationError> {
    let rows = sqlx::query(
        r"
        WITH candidates AS (
            SELECT id
            FROM ops.job
            WHERE status IN ('queued', 'retry')
              AND available_at <= now()
            ORDER BY available_at, created_at
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE ops.job job
           SET status = 'running',
               attempts = attempts + 1,
               locked_at = now(),
               locked_by = $2
          FROM candidates
         WHERE job.id = candidates.id
        RETURNING job.id, job.job_type, job.payload, job.attempts, job.max_attempts
        ",
    )
    .bind(limit.clamp(1, 100))
    .bind(worker_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    rows.into_iter()
        .map(|row| {
            Ok(ClaimedJob {
                id: row
                    .try_get("id")
                    .map_err(|_| ApplicationError::Persistence)?,
                job_type: row
                    .try_get("job_type")
                    .map_err(|_| ApplicationError::Persistence)?,
                payload: row
                    .try_get("payload")
                    .map_err(|_| ApplicationError::Persistence)?,
                attempts: row
                    .try_get("attempts")
                    .map_err(|_| ApplicationError::Persistence)?,
                max_attempts: row
                    .try_get("max_attempts")
                    .map_err(|_| ApplicationError::Persistence)?,
            })
        })
        .collect()
}

/// Completes a currently running job and clears its lease.
///
/// # Errors
///
/// Returns `Persistence` when the job is not running or `PostgreSQL` fails.
pub async fn complete_job(pool: &PgPool, job_id: Uuid) -> Result<(), ApplicationError> {
    let changed = sqlx::query(
        "UPDATE ops.job
            SET status = 'completed', locked_at = NULL, locked_by = NULL, last_error = NULL
          WHERE id = $1 AND status = 'running'",
    )
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    if changed.rows_affected() != 1 {
        return Err(ApplicationError::Persistence);
    }
    Ok(())
}

/// Moves a failed job to bounded retry or dead-letter state.
///
/// # Errors
///
/// Returns `Persistence` when `PostgreSQL` cannot apply the transition.
pub async fn fail_job(
    pool: &PgPool,
    job: &ClaimedJob,
    error_code: &str,
) -> Result<(), ApplicationError> {
    let status = if job.attempts >= job.max_attempts {
        "dead"
    } else {
        "retry"
    };
    sqlx::query(
        r"
        UPDATE ops.job
           SET status = $2,
               available_at = now() + make_interval(secs => LEAST(300, attempts * attempts)),
               locked_at = NULL,
               locked_by = NULL,
               last_error = $3
         WHERE id = $1 AND status = 'running'
        ",
    )
    .bind(job.id)
    .bind(status)
    .bind(error_code)
    .execute(pool)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    Ok(())
}

/// Marks a locked outbox batch delivered to the foundation test sink.
///
/// # Errors
///
/// Returns `Persistence` when `PostgreSQL` cannot lock or finalize the batch.
pub async fn deliver_outbox_batch(
    pool: &PgPool,
    worker_id: &str,
    limit: i64,
) -> Result<u64, ApplicationError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| ApplicationError::Persistence)?;
    let events = sqlx::query_scalar::<_, Uuid>(
        r"
        SELECT id
        FROM ops.outbox_event
        WHERE published_at IS NULL
          AND dead_at IS NULL
          AND available_at <= now()
        ORDER BY available_at, created_at
        FOR UPDATE SKIP LOCKED
        LIMIT $1
        ",
    )
    .bind(limit.clamp(1, 100))
    .fetch_all(&mut *transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    if events.is_empty() {
        transaction
            .commit()
            .await
            .map_err(|_| ApplicationError::Persistence)?;
        return Ok(0);
    }
    let changed = sqlx::query(
        r"
        UPDATE ops.outbox_event
           SET published_at = now(),
               delivery_attempts = delivery_attempts + 1,
               locked_at = now(),
               locked_by = $2
         WHERE id = ANY($1::uuid[])
        ",
    )
    .bind(&events)
    .bind(worker_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApplicationError::Persistence)?;
    transaction
        .commit()
        .await
        .map_err(|_| ApplicationError::Persistence)?;
    Ok(changed.rows_affected())
}
