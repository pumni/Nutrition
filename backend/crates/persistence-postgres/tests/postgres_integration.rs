use adapters::FixtureParser;
use application::{
    AnalysisMode, AnalysisOutcome, AnalysisRequest, AnalysisRevisionService, AnalysisSnapshot,
    AnalysisSnapshotReader, AnalyzeMeal, AnswerClarification, ApplicationError, BehaviorVersions,
    ClarificationAnswerRequest, CorrectAnalysis, CorrectionRequest, MealAnalysisService,
    ParserInvocationRecord, ParserTelemetrySink, PortionCorrection,
};
use domain::{NutrientCode, UserId};
use persistence_postgres::{
    PostgresAnalysisRepository, PostgresCatalogEvidenceProvider, PostgresParserTelemetrySink,
    PostgresPortionEvidenceProvider, active_catalog_release_id, claim_jobs, complete_job, connect,
    deliver_outbox_batch, fail_job, migrate, seed_foundation_fixture,
};
use rust_decimal::Decimal;
use std::{env, str::FromStr};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn contextual_analysis_is_persisted_and_replayed() {
    let database_url =
        env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required for integration test");
    let pool = connect(&database_url, 4)
        .await
        .expect("integration database must connect");
    migrate(&pool)
        .await
        .expect("integration migrations must apply");
    seed_foundation_fixture(&pool)
        .await
        .expect("foundation seed must apply idempotently");
    let versions = BehaviorVersions {
        catalog_release_id: active_catalog_release_id(&pool)
            .await
            .expect("active catalog release must exist"),
        ..BehaviorVersions::default()
    };
    let repository = PostgresAnalysisRepository::new(pool.clone());
    let food_evidence = PostgresCatalogEvidenceProvider::new(pool.clone());
    let portion_evidence = PostgresPortionEvidenceProvider::new(pool.clone());
    let service = MealAnalysisService::new(
        FixtureParser,
        food_evidence.clone(),
        portion_evidence.clone(),
        repository.clone(),
        versions.clone(),
        required_nutrients(),
    );
    let revision_service = AnalysisRevisionService::new(
        food_evidence,
        portion_evidence,
        repository.clone(),
        versions,
        required_nutrients(),
    );

    let outcome = service
        .execute(AnalysisRequest {
            text: "2 quả trứng gà luộc, 1 bát cơm trắng".to_owned(),
            locale: "vi-VN".to_owned(),
            mode: AnalysisMode::Balanced,
            idempotency: None,
            owner_id: Some(UserId::from_u128(0x0198_f100_0000_7000_8000_0000_0000_0098)),
        })
        .await
        .expect("PostgreSQL-backed analysis must complete");
    let AnalysisOutcome::Completed(snapshot) = outcome else {
        panic!("supported contextual analysis must complete");
    };

    let replayed = repository
        .find(snapshot.analysis_id)
        .await
        .expect("snapshot read must succeed")
        .expect("snapshot must exist");
    assert_contextual_snapshot(&snapshot, &replayed);

    let revision_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM analysis.analysis_revision WHERE meal_analysis_id = $1",
    )
    .bind(snapshot.analysis_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("revision count query must succeed");
    assert_eq!(revision_count, 1);

    let outbox_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ops.outbox_event WHERE aggregate_id = $1")
            .bind(snapshot.analysis_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("outbox count query must succeed");
    assert_eq!(outbox_count, 1);
    let assumed_item_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM analysis.analysis_item
          WHERE revision_id = $1
            AND resolution_status = 'resolved_with_assumption'",
    )
    .bind(snapshot.revision_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("resolution status query must succeed");
    assert_eq!(assumed_item_count, 2);
    assert!(
        repository
            .authorize_analysis(
                snapshot.analysis_id,
                UserId::from_u128(0x0198_f100_0000_7000_8000_0000_0000_0098)
            )
            .await
            .expect("owner authorization query must succeed")
    );
    assert!(
        !repository
            .authorize_analysis(
                snapshot.analysis_id,
                UserId::from_u128(0x0198_f100_0000_7000_8000_0000_0000_0097)
            )
            .await
            .expect("foreign authorization query must succeed")
    );

    assert_unknown_food_is_not_persisted(&service, &pool).await;
    assert_clarification_revision_flow(&service, &revision_service, &repository).await;
    assert_correction_revision_flow(&revision_service, &repository, &snapshot).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn parser_telemetry_persists_only_non_raw_metadata() {
    let database_url =
        env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required for integration test");
    let pool = connect(&database_url, 2)
        .await
        .expect("integration database must connect");
    migrate(&pool)
        .await
        .expect("integration migrations must apply");
    assert_parser_telemetry_is_non_raw(&pool).await;
}

async fn assert_parser_telemetry_is_non_raw(pool: &sqlx::PgPool) {
    PostgresParserTelemetrySink::new(pool.clone())
        .record(ParserInvocationRecord {
            provider: "integration-provider".to_owned(),
            model: "integration-model".to_owned(),
            prompt_version: "hosted-parser-0.1.0".to_owned(),
            schema_version: "parsed-meal-0.1.0".to_owned(),
            latency_ms: 42,
            retry_count: 1,
            input_tokens: Some(10),
            output_tokens: Some(20),
            output_sha256: Some("a".repeat(64)),
            status: "succeeded".to_owned(),
            error_code: None,
        })
        .await
        .expect("non-raw parser telemetry must persist");
    let persisted: (i64, i32, Option<i64>, Option<i64>, String) = sqlx::query_as(
        "SELECT latency_ms, retry_count, input_tokens, output_tokens, output_sha256
           FROM ops.parser_invocation
          WHERE provider = 'integration-provider'
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .expect("parser telemetry must be readable");
    assert_eq!(persisted, (42, 1, Some(10), Some(20), "a".repeat(64)));
    let sensitive_columns: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM information_schema.columns
          WHERE table_schema = 'ops'
            AND table_name = 'parser_invocation'
            AND column_name IN ('meal_text', 'raw_text', 'request', 'response', 'payload')",
    )
    .fetch_one(pool)
    .await
    .expect("telemetry schema inspection must succeed");
    assert_eq!(sensitive_columns, 0);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn worker_claim_retry_and_outbox_delivery_are_bounded() {
    let database_url =
        env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required for integration test");
    let pool = connect(&database_url, 4)
        .await
        .expect("integration database must connect");
    migrate(&pool)
        .await
        .expect("integration migrations must apply");
    let noop_id = uuid::Uuid::now_v7();
    let failing_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO ops.job (id, job_type, payload, status, max_attempts)
         VALUES ($1, 'foundation_noop', '{}', 'queued', 3),
                ($2, 'unsupported_fixture', '{}', 'queued', 1)",
    )
    .bind(noop_id)
    .bind(failing_id)
    .execute(&pool)
    .await
    .expect("fixture jobs must insert");
    let claimed = claim_jobs(&pool, "integration-worker", 10)
        .await
        .expect("jobs must claim");
    let noop = claimed
        .iter()
        .find(|job| job.id == noop_id)
        .expect("noop job must be claimed");
    complete_job(&pool, noop.id)
        .await
        .expect("noop job must complete");
    let failing = claimed
        .iter()
        .find(|job| job.id == failing_id)
        .expect("failing job must be claimed");
    fail_job(&pool, failing, "fixture_failure")
        .await
        .expect("failing job must transition");
    let failing_status: String = sqlx::query_scalar("SELECT status FROM ops.job WHERE id = $1")
        .bind(failing_id)
        .fetch_one(&pool)
        .await
        .expect("job status query must succeed");
    assert_eq!(failing_status, "dead");

    let outbox_id = uuid::Uuid::now_v7();
    let aggregate_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO ops.outbox_event (id, aggregate_type, aggregate_id, event_type, payload)
         VALUES ($1, 'integration_fixture', $2, 'integration_fixture.ready', '{}')",
    )
    .bind(outbox_id)
    .bind(aggregate_id)
    .execute(&pool)
    .await
    .expect("outbox fixture must insert");

    let delivered = deliver_outbox_batch(&pool, "integration-worker", 100)
        .await
        .expect("outbox batch must deliver");
    assert!(delivered > 0);
    let fixture_published: bool = sqlx::query_scalar(
        "SELECT published_at IS NOT NULL FROM ops.outbox_event WHERE id = $1",
    )
    .bind(outbox_id)
    .fetch_one(&pool)
    .await
    .expect("outbox fixture delivery must be readable");
    assert!(fixture_published);
}

fn assert_contextual_snapshot(expected: &AnalysisSnapshot, actual: &AnalysisSnapshot) {
    assert_eq!(
        serde_json::to_value(expected).expect("snapshot serializes"),
        serde_json::to_value(actual).expect("replayed snapshot serializes")
    );
    let protein = actual
        .calculation
        .totals
        .iter()
        .find(|total| total.nutrient.as_str() == "protein_g")
        .expect("protein total must exist");
    assert_eq!(protein.amount, Some(decimal_value("16.635")));

    let energy = actual
        .calculation
        .totals
        .iter()
        .find(|total| total.nutrient.as_str() == "energy_kcal")
        .expect("energy total must exist");
    assert_eq!(energy.lower_amount, Some(decimal_value("295.5")));
    assert_eq!(energy.upper_amount, Some(decimal_value("446")));

    assert_eq!(actual.items[0].estimated_mass_g, decimal_value("100"));
    assert_eq!(actual.items[0].lower_mass_g, Some(decimal_value("90")));
    assert_eq!(actual.items[0].upper_mass_g, Some(decimal_value("120")));
    assert!(actual.items[0].portion_observation_id.is_some());
    assert_eq!(actual.items[1].estimated_mass_g, decimal_value("150"));
    assert_eq!(actual.items[1].lower_mass_g, Some(decimal_value("120")));
    assert_eq!(actual.items[1].upper_mass_g, Some(decimal_value("200")));
    assert!(actual.items[1].portion_observation_id.is_some());
}

async fn assert_unknown_food_is_not_persisted(
    service: &(impl AnalyzeMeal + ?Sized),
    pool: &sqlx::PgPool,
) {
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM analysis.meal_analysis")
        .fetch_one(pool)
        .await
        .expect("analysis count query must succeed");
    let error = service
        .execute(AnalysisRequest {
            text: "100 g món không tồn tại".to_owned(),
            locale: "vi-VN".to_owned(),
            mode: AnalysisMode::Balanced,
            idempotency: None,
            owner_id: None,
        })
        .await
        .expect_err("unknown food must not be persisted");
    assert!(matches!(error, ApplicationError::InsufficientEvidence(_)));
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM analysis.meal_analysis")
        .fetch_one(pool)
        .await
        .expect("analysis count query must succeed");
    assert_eq!(before, after);
}

async fn assert_clarification_revision_flow(
    analyzer: &(impl AnalyzeMeal + ?Sized),
    revision_service: &(impl AnswerClarification + ?Sized),
    repository: &PostgresAnalysisRepository,
) {
    let outcome = analyzer
        .execute(AnalysisRequest {
            text: "1 ly cơm trắng".to_owned(),
            locale: "vi-VN".to_owned(),
            mode: AnalysisMode::Balanced,
            idempotency: None,
            owner_id: None,
        })
        .await
        .expect("unsupported known portion should request clarification");
    let AnalysisOutcome::NeedsClarification(pending) = outcome else {
        panic!("unsupported known portion must request clarification");
    };
    let answer = ClarificationAnswerRequest {
        expected_revision_id: pending.revision_id,
        question_id: pending.question.id,
        option_id: "unit:bát".to_owned(),
        mass_g: None,
    };
    let completed = revision_service
        .answer(pending.analysis_id, answer.clone())
        .await
        .expect("clarification answer must create a completed revision");
    assert_eq!(completed.revision_number, 2);
    assert_eq!(completed.items[0].estimated_mass_g, decimal_value("150"));
    let stale = revision_service
        .answer(pending.analysis_id, answer)
        .await
        .expect_err("replayed stale answer must fail");
    assert!(matches!(stale, ApplicationError::StaleClarification));
    let first_revision = repository
        .find_revision(pending.analysis_id, 1)
        .await
        .expect("history read must succeed")
        .expect("clarification revision must remain");
    assert_eq!(first_revision["status"], "needs_clarification");
}

async fn assert_correction_revision_flow(
    revision_service: &(impl CorrectAnalysis + ?Sized),
    repository: &PostgresAnalysisRepository,
    original: &AnalysisSnapshot,
) {
    let request = CorrectionRequest {
        base_revision_id: original.revision_id,
        item_corrections: vec![PortionCorrection {
            item_index: 0,
            quantity: Decimal::ONE,
            unit: "quả".to_owned(),
        }],
        idempotency: None,
    };
    let corrected = revision_service
        .correct(original.analysis_id, request.clone())
        .await
        .expect("correction must append a revision");
    assert_eq!(corrected.revision_number, 2);
    assert_eq!(corrected.items[0].estimated_mass_g, decimal_value("50"));
    let stale = revision_service
        .correct(original.analysis_id, request)
        .await
        .expect_err("stale base revision must fail");
    assert!(matches!(stale, ApplicationError::RevisionConflict));
    let original_history = repository
        .find_revision(original.analysis_id, 1)
        .await
        .expect("history read must succeed")
        .expect("original revision must remain");
    assert_eq!(original_history["revision_number"], 1);
}

fn decimal_value(value: &str) -> Decimal {
    Decimal::from_str(value).expect("expected decimal is valid")
}

fn required_nutrients() -> Vec<NutrientCode> {
    ["energy_kcal", "protein_g", "carbohydrate_g", "fat_g"]
        .into_iter()
        .map(|code| NutrientCode::new(code).expect("fixture nutrient code is valid"))
        .collect()
}
