//! Ownership-scoped query responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

impl PostgresAnalysisRepository {
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
        crate::telemetry::observe_db_future("analysis_authorize", async {
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
        })
        .await
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
pub(crate) fn safe_clarification_context(
    context: &application::ClarificationContext,
) -> Result<Value, ApplicationError> {
    let mut value = serde_json::to_value(context).map_err(|_| ApplicationError::Persistence)?;
    redact_persisted_value(&mut value);
    Ok(value)
}
