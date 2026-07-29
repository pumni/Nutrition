use adapters::FixtureParser;
use application::{
    AnalysisMode, AnalysisRequest, AnalysisSnapshotReader, AnalyzeMeal, ApplicationError,
    BehaviorVersions, DirectAnalysisService,
};
use domain::NutrientCode;
use persistence_postgres::{
    PostgresAnalysisRepository, PostgresCatalogEvidenceProvider, active_catalog_release_id,
    connect, migrate, seed_foundation_fixture,
};
use rust_decimal::Decimal;
use std::{env, str::FromStr};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn exact_analysis_is_persisted_and_replayed() {
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
    let service = DirectAnalysisService::new(
        FixtureParser,
        PostgresCatalogEvidenceProvider::new(pool.clone()),
        repository.clone(),
        versions,
        required_nutrients(),
    );

    let snapshot = service
        .execute(AnalysisRequest {
            text: "100 g trứng gà luộc, 150 g cơm trắng".to_owned(),
            locale: "vi-VN".to_owned(),
            mode: AnalysisMode::Balanced,
        })
        .await
        .expect("PostgreSQL-backed analysis must complete");

    let replayed = repository
        .find(snapshot.analysis_id)
        .await
        .expect("snapshot read must succeed")
        .expect("snapshot must exist");
    assert_eq!(
        serde_json::to_value(&snapshot).expect("snapshot serializes"),
        serde_json::to_value(&replayed).expect("replayed snapshot serializes")
    );

    let protein = replayed
        .calculation
        .totals
        .iter()
        .find(|total| total.nutrient.as_str() == "protein_g")
        .expect("protein total must exist");
    assert_eq!(
        protein.amount,
        Some(Decimal::from_str("16.635").expect("expected decimal is valid"))
    );

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

    let analysis_count_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM analysis.meal_analysis")
            .fetch_one(&pool)
            .await
            .expect("analysis count query must succeed");
    let unknown_error = service
        .execute(AnalysisRequest {
            text: "100 g món không tồn tại".to_owned(),
            locale: "vi-VN".to_owned(),
            mode: AnalysisMode::Balanced,
        })
        .await
        .expect_err("unknown food must not be persisted");
    assert!(matches!(
        unknown_error,
        ApplicationError::InsufficientEvidence(_)
    ));
    let analysis_count_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM analysis.meal_analysis")
            .fetch_one(&pool)
            .await
            .expect("analysis count query must succeed");
    assert_eq!(analysis_count_before, analysis_count_after);
}

fn required_nutrients() -> Vec<NutrientCode> {
    ["energy_kcal", "protein_g", "carbohydrate_g", "fat_g"]
        .into_iter()
        .map(|code| NutrientCode::new(code).expect("fixture nutrient code is valid"))
        .collect()
}
