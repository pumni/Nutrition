use adapters::FixtureParser;
use application::{
    AnalysisMode, AnalysisRequest, AnalysisSnapshot, AnalysisSnapshotReader, AnalyzeMeal,
    ApplicationError, BehaviorVersions, MealAnalysisService,
};
use domain::NutrientCode;
use persistence_postgres::{
    PostgresAnalysisRepository, PostgresCatalogEvidenceProvider, PostgresPortionEvidenceProvider,
    active_catalog_release_id, connect, migrate, seed_foundation_fixture,
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
    let service = MealAnalysisService::new(
        FixtureParser,
        PostgresCatalogEvidenceProvider::new(pool.clone()),
        PostgresPortionEvidenceProvider::new(pool.clone()),
        repository.clone(),
        versions,
        required_nutrients(),
    );

    let snapshot = service
        .execute(AnalysisRequest {
            text: "2 quả trứng gà luộc, 1 bát cơm trắng".to_owned(),
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

    assert_rejections_are_not_persisted(&service, &pool).await;
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

async fn assert_rejections_are_not_persisted(
    service: &(impl AnalyzeMeal + ?Sized),
    pool: &sqlx::PgPool,
) {
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM analysis.meal_analysis")
        .fetch_one(pool)
        .await
        .expect("analysis count query must succeed");
    for text in ["100 g món không tồn tại", "1 ly cơm trắng"] {
        let error = service
            .execute(AnalysisRequest {
                text: text.to_owned(),
                locale: "vi-VN".to_owned(),
                mode: AnalysisMode::Balanced,
            })
            .await
            .expect_err("insufficient evidence must not be persisted");
        assert!(matches!(error, ApplicationError::InsufficientEvidence(_)));
    }
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM analysis.meal_analysis")
        .fetch_one(pool)
        .await
        .expect("analysis count query must succeed");
    assert_eq!(before, after);
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
