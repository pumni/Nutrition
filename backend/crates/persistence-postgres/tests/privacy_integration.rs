use domain::UserId;
use persistence_postgres::{
    PrivacyDeletionReceipt, connect, delete_user_data, export_user_data, migrate,
    run_privacy_retention,
};
use serde_json::json;
use sqlx::PgPool;
use std::env;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn privacy_export_redacts_and_delete_purges_only_user_owned_data() {
    let pool = setup_database().await;
    let user_id = Uuid::now_v7();
    let analysis_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let catalog_release_id = Uuid::now_v7();
    let request_reference = "privacy-test-request";

    insert_fixture(&pool, user_id, analysis_id, revision_id, catalog_release_id).await;

    let export = export_user_data(&pool, UserId::from_uuid(user_id))
        .await
        .expect("privacy export must succeed");
    let export_text = serde_json::to_string(&export).expect("export must serialize");
    assert_eq!(export["export_version"], "user-data-export-v1");
    assert!(!export_text.contains("raw meal phrase"));
    assert!(!export_text.contains("Authorization: Bearer"));
    assert!(!export_text.contains("source_text"));

    let receipt = delete_user_data(&pool, UserId::from_uuid(user_id), request_reference)
        .await
        .expect("user deletion must succeed");
    assert_eq!(
        receipt,
        PrivacyDeletionReceipt {
            event_type: "privacy.deletion_completed",
            deleted_at: receipt.deleted_at.clone(),
            request_reference: request_reference.to_owned(),
        }
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM analysis.meal_analysis WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("analysis count must be readable"),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM auth.external_identity WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("identity count must be readable"),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM ops.audit_event
              WHERE action = 'privacy.deletion_completed' AND target_id = $1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("deletion receipt count must be readable"),
        1
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn privacy_retention_removes_old_telemetry_and_audit_metadata() {
    let pool = setup_database().await;
    sqlx::query(
        "INSERT INTO ops.parser_invocation
            (id, provider, model, prompt_version, schema_version, latency_ms, retry_count,
             output_sha256, status, created_at)
         VALUES ($1, 'test', 'test', 'test', 'test', 1, 0, $2, 'succeeded', now() - interval '31 days')",
    )
    .bind(Uuid::now_v7())
    .bind("a".repeat(64))
    .execute(&pool)
    .await
    .expect("old telemetry fixture must insert");
    let old_audit_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO ops.audit_event (id, action, target_type, target_id, metadata, created_at)
         VALUES ($1, 'test.old', 'test', $2, '{}', now() - interval '366 days')",
    )
    .bind(old_audit_id)
    .bind(Uuid::now_v7())
    .execute(&pool)
    .await
    .expect("old audit fixture must insert");

    let report = run_privacy_retention(&pool)
        .await
        .expect("privacy retention must succeed");
    assert!(report.deleted_parser_telemetry >= 1);
    assert!(report.deleted_audit_events >= 1);
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT NOT EXISTS (SELECT 1 FROM ops.audit_event WHERE id = $1)"
        )
        .bind(old_audit_id)
        .fetch_one(&pool)
        .await
        .expect("audit retention result must be readable")
    );
}

async fn setup_database() -> PgPool {
    let database_url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let pool = connect(&database_url, 4)
        .await
        .expect("privacy database must connect");
    migrate(&pool).await.expect("privacy migrations must apply");
    pool
}

async fn insert_fixture(
    pool: &PgPool,
    user_id: Uuid,
    analysis_id: Uuid,
    revision_id: Uuid,
    catalog_release_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO catalog.catalog_release
            (id, version, status, manifest, checksum_sha256, created_by)
         VALUES ($1, $2, 'staged', '{}', $3, $4)",
    )
    .bind(catalog_release_id)
    .bind(format!("privacy-test-{catalog_release_id}"))
    .bind("a".repeat(64))
    .bind(Uuid::now_v7())
    .execute(pool)
    .await
    .expect("catalog fixture must insert");
    sqlx::query(
        "INSERT INTO analysis.meal_analysis
            (id, user_id, locale, status)
         VALUES ($1, $2, 'vi-VN', 'completed')",
    )
    .bind(analysis_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("analysis fixture must insert");
    sqlx::query(
        "INSERT INTO analysis.analysis_revision
            (id, meal_analysis_id, revision_number, revision_reason, application_version,
             parser_schema_version, prompt_version, model_provider_version, normalization_version,
             resolution_policy_version, portion_policy_version, composition_policy_version,
             calculation_engine_version, catalog_release_id, result_status, quality_label,
             result_snapshot, snapshot_hash)
         VALUES ($1, $2, 1, 'privacy-test', 'test', 'test', 'test', 'test', 'test', 'test',
                 'test', 'test', 'test', $3, 'building', 'insufficient', NULL, NULL)",
    )
    .bind(revision_id)
    .bind(analysis_id)
    .bind(catalog_release_id)
    .execute(pool)
    .await
    .expect("revision fixture must insert");
    sqlx::query(
        "INSERT INTO analysis.analysis_item
            (id, revision_id, item_index, source_text, parsed_payload, resolution_status, evidence_quality)
         VALUES ($1, $2, 0, 'raw meal phrase', '{}', 'unresolved', 'U')",
    )
    .bind(Uuid::now_v7())
    .bind(revision_id)
    .execute(pool)
    .await
    .expect("analysis item fixture must insert");
    sqlx::query(
        "UPDATE analysis.analysis_revision
            SET result_status = 'completed', result_snapshot = $2, snapshot_hash = $3
          WHERE id = $1",
    )
    .bind(revision_id)
    .bind(
        json!({"items": [{"source_text": "raw meal phrase", "source_spans": ["raw meal phrase"]}]}),
    )
    .bind("b".repeat(64))
    .execute(pool)
    .await
    .expect("revision fixture must finalize");
    sqlx::query("UPDATE analysis.meal_analysis SET current_revision_id = $2 WHERE id = $1")
        .bind(analysis_id)
        .bind(revision_id)
        .execute(pool)
        .await
        .expect("analysis current revision must be linked");
    sqlx::query(
        "INSERT INTO auth.external_identity (issuer, subject, user_id)
         VALUES ('https://issuer.example/', 'subject-privacy-test', $1)",
    )
    .bind(user_id)
    .execute(pool)
    .await
    .expect("external identity fixture must insert");
    sqlx::query(
        "INSERT INTO app.idempotency_record
            (scope_key, idempotency_key, request_hash, expires_at)
         VALUES ($1, 'privacy-test', $2, now() + interval '1 day')",
    )
    .bind(format!("user:{user_id}:create"))
    .bind("c".repeat(64))
    .execute(pool)
    .await
    .expect("idempotency fixture must insert");
}
