//! Local-only performance evidence harness. This module is compiled only for tests.

use adapters::{
    APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS, APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD,
    APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES, APPROVED_HOSTED_TIMEOUT_MS, FixtureParser,
    HOSTED_PROMPT_VERSION, HostedMealParser, HostedParserConfig, ModelIdentity, ProviderIdentity,
    StrictJsonSchema, StructuredGenerationRequest, StructuredGenerationResponse, StructuredModel,
    StructuredModelError, StructuredModelErrorClassification, StructuredResponseMetadata,
    UntrustedInput,
};
use application::{
    AnalysisRevisionService, AnalysisSnapshotReader, AnalyzeMeal, AnswerClarification,
    ApplicationError, BehaviorVersions, CorrectAnalysis, FoodEvidenceProvider, MealAnalysisService,
    MealTextParser, ParsedMealItem, PortionEvidenceProvider, PortionSuggestion,
    ResolvedFoodEvidence, ResolvedPortionEvidence,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, header},
};
use domain::{CatalogReleaseId, NutrientCode};
use persistence_postgres::{
    PostgresAnalysisRepository, PostgresCatalogEvidenceProvider, PostgresPortionEvidenceProvider,
    active_catalog_release_id,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::task::JoinSet;
use tower::ServiceExt;
use uuid::Uuid;

const REPORT_SCHEMA_VERSION: &str = "performance-baseline-0.1.0";
const API_DATABASE_POOL_SIZE: u32 = 8;
const DB_SINGLE_ITEM_TEXT: &str = "2 quả trứng gà luộc";
const DB_MULTI_ITEM_TEXT: &str = "2 quả trứng gà luộc, 1 bát cơm trắng";
const HOSTED_FAKE_TEXT: &str = "2 quả trứng gà luộc";
const WARMUP_REQUEST_COUNT: usize = 10;
const DEFAULT_REQUEST_COUNT: usize = 100;
const DEFAULT_CONCURRENCY: usize = 4;
const FAKE_PROVIDER_DELAY_MS: u64 = 25;
const FAKE_TRANSIENT_ERROR_EVERY: usize = 17;
const ISSUE_20_P95_ABSOLUTE_THRESHOLD_MS: f64 = 25.0;
const ISSUE_20_P95_RELATIVE_THRESHOLD: f64 = 0.25;
const ISSUE_20_P99_ABSOLUTE_THRESHOLD_MS: f64 = 50.0;
const ISSUE_20_P99_RELATIVE_THRESHOLD: f64 = 0.25;
const ISSUE_20_EVIDENCE_ABSOLUTE_THRESHOLD_MS: f64 = 10.0;
const ISSUE_20_EVIDENCE_P95_DELTA_FRACTION: f64 = 0.5;
const MAXIMUM_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct PerformanceBaselineReport {
    schema_version: String,
    scope: ReportScope,
    percentile_definition: String,
    environment: EnvironmentMetadata,
    db_oriented_api: DbOrientedReport,
    hosted_parser_analysis: HostedParserReport,
    issue_20_decision: Issue20DecisionReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[allow(clippy::struct_excessive_bools)]
struct ReportScope {
    local_ci_evidence_only: bool,
    production_credentials_used: bool,
    real_hosted_provider_called: bool,
    production_slo: bool,
    production_capacity_claim: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct EnvironmentMetadata {
    source_revision: String,
    operating_system: String,
    architecture: String,
    rustc_version: String,
    build_profile: String,
    logical_cpu_count: Option<usize>,
    postgresql_server_version: String,
    dataset: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct WorkloadReport {
    scenario: String,
    operation: String,
    request_count: usize,
    warmup_request_count: usize,
    concurrency: usize,
    elapsed_ms: f64,
    throughput_requests_per_second: f64,
    latency: LatencyDistribution,
    status_distribution: BTreeMap<String, usize>,
    error_code_distribution: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct LatencyDistribution {
    sample_count: usize,
    min_ms: f64,
    max_ms: f64,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct DbOrientedReport {
    request_path: String,
    parser: String,
    scenarios: Vec<DbScenarioReport>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct DbScenarioReport {
    item_count: usize,
    workload: WorkloadReport,
    database: DatabaseReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct DatabaseReport {
    configured_max_connections: u32,
    pool_before: PoolSnapshot,
    pool_during: PoolRange,
    pool_after: PoolSnapshot,
    evidence_resolution: EvidenceResolutionReport,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
struct PoolSnapshot {
    size: u32,
    idle: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
struct PoolRange {
    observation_count: usize,
    minimum_size: u32,
    maximum_size: u32,
    minimum_idle: usize,
    maximum_idle: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct EvidenceResolutionReport {
    food: OperationTimingReport,
    portion: OperationTimingReport,
    portion_suggestions: OperationTimingReport,
    total_calls: usize,
    total_observed_ms: f64,
    mean_observed_ms_per_request: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct OperationTimingReport {
    calls: usize,
    total_ms: f64,
    mean_ms: f64,
    latency: Option<LatencyDistribution>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct HostedParserReport {
    request_path: String,
    parser: String,
    workload: WorkloadReport,
    database: DatabaseReport,
    fake_profile: FakeProfileReport,
    fake_model_call_count: usize,
    fake_provider_latency: LatencyDistribution,
    derived_backend_overhead_observation: DerivedOverheadReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct FakeProfileReport {
    fake_model: String,
    fixed_delay_ms: u64,
    transient_error_every_nth_call: usize,
    transient_error_code: String,
    external_credential_required: bool,
    real_provider_called: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct DerivedOverheadReport {
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    method: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Issue20DecisionReport {
    threshold_predeclared_at_revision: String,
    outcome: String,
    next_action: String,
    triggered: bool,
    threshold: Issue20Threshold,
    observed: Issue20Measurements,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Issue20Threshold {
    comparison: String,
    minimum_p95_increase_ms: f64,
    minimum_p95_increase_fraction: f64,
    minimum_p99_increase_ms: f64,
    minimum_p99_increase_fraction: f64,
    minimum_added_evidence_ms_per_request: f64,
    minimum_added_evidence_fraction_of_p95_increase: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Issue20Measurements {
    p95_increase_ms: f64,
    p99_increase_ms: f64,
    added_evidence_ms_per_request: f64,
    added_evidence_fraction_of_p95_increase: f64,
    one_item_evidence_calls_per_request: f64,
    multi_item_evidence_calls_per_request: f64,
    both_workloads_succeeded: bool,
    expected_round_trip_counts_observed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeFailureKind {
    Transient,
    Permanent,
}

#[derive(Clone, Debug)]
struct FakeProfile {
    fixed_delay: Duration,
    error_every_nth_call: Option<usize>,
    failure_kind: Option<FakeFailureKind>,
}

struct DeterministicStructuredModel {
    profile: FakeProfile,
    calls: AtomicUsize,
    observed_latency_ms: Mutex<Vec<f64>>,
}

impl DeterministicStructuredModel {
    fn new(profile: FakeProfile) -> Self {
        Self {
            profile,
            calls: AtomicUsize::new(0),
            observed_latency_ms: Mutex::new(Vec::new()),
        }
    }

    fn reset(&self) {
        self.calls.store(0, Ordering::SeqCst);
        self.observed_latency_ms
            .lock()
            .expect("fake timing mutex is not poisoned")
            .clear();
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn observed_latency_ms(&self) -> Vec<f64> {
        self.observed_latency_ms
            .lock()
            .expect("fake timing mutex is not poisoned")
            .clone()
    }
}

#[async_trait]
impl StructuredModel for DeterministicStructuredModel {
    async fn generate(
        &self,
        _request: &StructuredGenerationRequest,
        _maximum_response_bytes: usize,
    ) -> Result<StructuredGenerationResponse, StructuredModelError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let started = Instant::now();
        tokio::time::sleep(self.profile.fixed_delay).await;
        self.observed_latency_ms
            .lock()
            .expect("fake timing mutex is not poisoned")
            .push(started.elapsed().as_secs_f64() * 1_000.0);

        if self
            .profile
            .error_every_nth_call
            .is_some_and(|every| call.is_multiple_of(every))
        {
            let classification = match self
                .profile
                .failure_kind
                .unwrap_or(FakeFailureKind::Transient)
            {
                FakeFailureKind::Transient => StructuredModelErrorClassification::Transient,
                FakeFailureKind::Permanent => StructuredModelErrorClassification::Permanent,
            };
            let code = match classification {
                StructuredModelErrorClassification::Transient => "benchmark_transient",
                StructuredModelErrorClassification::Permanent => "benchmark_permanent",
                StructuredModelErrorClassification::CapacityRejected => unreachable!(),
            };
            return Err(StructuredModelError::new(classification, code));
        }

        Ok(StructuredGenerationResponse {
            output: json!({
                "language": "vi",
                "items": [{
                    "source_text": "2 quả trứng gà luộc",
                    "food_phrase": "trứng gà luộc",
                    "quantity": 2,
                    "unit_phrase": "quả",
                    "modifiers": ["luộc"]
                }],
                "warnings": []
            }),
            metadata: StructuredResponseMetadata::default(),
        })
    }
}

#[derive(Clone, Debug, Default)]
struct EvidenceTimings {
    food_ms: Vec<f64>,
    portion_ms: Vec<f64>,
    suggestions_ms: Vec<f64>,
    pool_samples: Vec<PoolSnapshot>,
}

type SharedEvidenceTimings = Arc<Mutex<EvidenceTimings>>;

#[derive(Clone)]
struct MeasuredFoodEvidenceProvider {
    inner: PostgresCatalogEvidenceProvider,
    pool: PgPool,
    timings: SharedEvidenceTimings,
}

#[async_trait]
impl FoodEvidenceProvider for MeasuredFoodEvidenceProvider {
    async fn resolve_food(
        &self,
        locale: &str,
        item: &ParsedMealItem,
    ) -> Result<ResolvedFoodEvidence, ApplicationError> {
        record_pool_sample(&self.timings, &self.pool);
        let started = Instant::now();
        let result = self.inner.resolve_food(locale, item).await;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let mut timings = self
            .timings
            .lock()
            .expect("evidence timing mutex is not poisoned");
        timings.food_ms.push(elapsed_ms);
        timings.pool_samples.push(pool_snapshot(&self.pool));
        result
    }
}

#[derive(Clone)]
struct MeasuredPortionEvidenceProvider {
    inner: PostgresPortionEvidenceProvider,
    pool: PgPool,
    timings: SharedEvidenceTimings,
}

#[async_trait]
impl PortionEvidenceProvider for MeasuredPortionEvidenceProvider {
    async fn resolve_portion(
        &self,
        locale: &str,
        item: &ParsedMealItem,
        food_id: domain::FoodId,
    ) -> Result<ResolvedPortionEvidence, ApplicationError> {
        record_pool_sample(&self.timings, &self.pool);
        let started = Instant::now();
        let result = self.inner.resolve_portion(locale, item, food_id).await;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let mut timings = self
            .timings
            .lock()
            .expect("evidence timing mutex is not poisoned");
        timings.portion_ms.push(elapsed_ms);
        timings.pool_samples.push(pool_snapshot(&self.pool));
        result
    }

    async fn suggestions(
        &self,
        locale: &str,
        food_id: domain::FoodId,
    ) -> Result<Vec<PortionSuggestion>, ApplicationError> {
        record_pool_sample(&self.timings, &self.pool);
        let started = Instant::now();
        let result = self.inner.suggestions(locale, food_id).await;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let mut timings = self
            .timings
            .lock()
            .expect("evidence timing mutex is not poisoned");
        timings.suggestions_ms.push(elapsed_ms);
        timings.pool_samples.push(pool_snapshot(&self.pool));
        result
    }
}

fn record_pool_sample(timings: &SharedEvidenceTimings, pool: &PgPool) {
    timings
        .lock()
        .expect("evidence timing mutex is not poisoned")
        .pool_samples
        .push(pool_snapshot(pool));
}

fn pool_snapshot(pool: &PgPool) -> PoolSnapshot {
    PoolSnapshot {
        size: pool.size(),
        idle: pool.num_idle(),
    }
}

fn clear_evidence_timings(timings: &SharedEvidenceTimings) {
    *timings
        .lock()
        .expect("evidence timing mutex is not poisoned") = EvidenceTimings::default();
}

fn compose_router<P>(
    pool: PgPool,
    catalog_release_id: CatalogReleaseId,
    parser: P,
    prompt_version: &str,
    model_provider_version: &str,
    timings: SharedEvidenceTimings,
) -> Router
where
    P: MealTextParser + 'static,
{
    let repository = PostgresAnalysisRepository::new(pool.clone());
    let food_evidence = MeasuredFoodEvidenceProvider {
        inner: PostgresCatalogEvidenceProvider::new(pool.clone(), catalog_release_id),
        pool: pool.clone(),
        timings: Arc::clone(&timings),
    };
    let portion_evidence = MeasuredPortionEvidenceProvider {
        inner: PostgresPortionEvidenceProvider::new(pool.clone(), catalog_release_id),
        pool: pool.clone(),
        timings,
    };
    let versions = BehaviorVersions {
        parser_schema_version: adapters::PARSER_SCHEMA_VERSION.to_owned(),
        prompt_version: prompt_version.to_owned(),
        model_provider_version: model_provider_version.to_owned(),
        catalog_release_id,
        ..BehaviorVersions::default()
    };
    let nutrients = ["energy_kcal", "protein_g", "carbohydrate_g", "fat_g"]
        .into_iter()
        .map(|code| NutrientCode::new(code).expect("built-in nutrient code is valid"))
        .collect();
    let analyzer = MealAnalysisService::new(
        parser,
        food_evidence.clone(),
        portion_evidence.clone(),
        repository.clone(),
        versions.clone(),
        nutrients,
    );
    let revision_service = Arc::new(AnalysisRevisionService::new(
        food_evidence,
        portion_evidence,
        repository.clone(),
        versions,
        ["energy_kcal", "protein_g", "carbohydrate_g", "fat_g"]
            .into_iter()
            .map(|code| NutrientCode::new(code).expect("built-in nutrient code is valid"))
            .collect(),
    ));
    crate::router::build_router(crate::app::AppState {
        authenticator: crate::auth::Authenticator::Development,
        analyzer: Arc::new(analyzer) as Arc<dyn AnalyzeMeal>,
        clarification: revision_service.clone() as Arc<dyn AnswerClarification>,
        correction: revision_service as Arc<dyn CorrectAnalysis>,
        reader: Arc::new(repository.clone()) as Arc<dyn AnalysisSnapshotReader>,
        repository,
        pool,
        cursor_hmac_secret: Arc::new(vec![b'c'; 32]),
    })
}

#[derive(Clone, Debug)]
struct ApiSample {
    elapsed_ms: f64,
    status: String,
    error_code: Option<String>,
}

async fn send_api_request(router: Router, text: String, idempotency_key: String) -> ApiSample {
    let body = serde_json::to_vec(&json!({"text": text, "locale": "vi-VN"}))
        .expect("benchmark request serializes");
    let request = Request::builder()
        .method("POST")
        .uri("/v1/nutrition/analyses")
        .header(
            header::AUTHORIZATION,
            "Bearer dev:0198f100-0000-7000-8000-000000000098",
        )
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", idempotency_key)
        .body(Body::from(body))
        .expect("benchmark request is valid");
    let started = Instant::now();
    let response = router
        .oneshot(request)
        .await
        .expect("in-process API router returns a response");
    let status = response.status().as_u16().to_string();
    let body = to_bytes(response.into_body(), MAXIMUM_RESPONSE_BYTES)
        .await
        .expect("API response body is bounded");
    let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let error_code = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|value| value.get("error")?.get("code")?.as_str().map(str::to_owned))
        .filter(|code| is_safe_error_code(code));
    ApiSample {
        elapsed_ms,
        status,
        error_code,
    }
}

fn is_safe_error_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

async fn run_requests(
    router: &Router,
    text: &str,
    request_count: usize,
    concurrency: usize,
    run_id: &str,
    scenario: &str,
    phase: &str,
) -> Vec<ApiSample> {
    let mut samples = Vec::with_capacity(request_count);
    let mut start = 0;
    while start < request_count {
        let end = (start + concurrency).min(request_count);
        let mut requests = JoinSet::new();
        for index in start..end {
            let app = router.clone();
            let text = text.to_owned();
            let idempotency_key = format!("baseline-{run_id}-{scenario}-{phase}-{index}");
            requests.spawn(async move { send_api_request(app, text, idempotency_key).await });
        }
        while let Some(sample) = requests.join_next().await {
            samples.push(sample.expect("benchmark request task completed"));
        }
        start = end;
    }
    samples
}

fn usize_as_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).expect("bounded baseline count fits in u32"))
}

fn latency_distribution(samples: &[f64]) -> LatencyDistribution {
    assert!(!samples.is_empty(), "latency distribution requires samples");
    assert!(
        samples
            .iter()
            .all(|sample| sample.is_finite() && *sample >= 0.0),
        "latency samples must be finite and non-negative"
    );
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let nearest_rank = |percent: usize| {
        let rank = sorted.len().saturating_mul(percent).div_ceil(100).max(1);
        sorted[rank - 1]
    };
    let total_ms = sorted.iter().sum::<f64>();
    LatencyDistribution {
        sample_count: samples.len(),
        min_ms: sorted[0],
        max_ms: sorted[sorted.len() - 1],
        mean_ms: total_ms / usize_as_f64(samples.len()),
        p50_ms: nearest_rank(50),
        p95_ms: nearest_rank(95),
        p99_ms: nearest_rank(99),
    }
}

fn workload_report(
    scenario: &str,
    request_count: usize,
    warmup_request_count: usize,
    concurrency: usize,
    elapsed: Duration,
    samples: &[ApiSample],
) -> WorkloadReport {
    let mut status_distribution = BTreeMap::new();
    let mut error_code_distribution = BTreeMap::new();
    let mut latencies = Vec::with_capacity(samples.len());
    for sample in samples {
        *status_distribution
            .entry(sample.status.clone())
            .or_insert(0) += 1;
        if let Some(error_code) = &sample.error_code {
            *error_code_distribution
                .entry(error_code.clone())
                .or_insert(0) += 1;
        }
        latencies.push(sample.elapsed_ms);
    }
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;
    WorkloadReport {
        scenario: scenario.to_owned(),
        operation: "POST /v1/nutrition/analyses via in-process Axum router".to_owned(),
        request_count,
        warmup_request_count,
        concurrency,
        elapsed_ms,
        throughput_requests_per_second: usize_as_f64(request_count) / elapsed.as_secs_f64(),
        latency: latency_distribution(&latencies),
        status_distribution,
        error_code_distribution,
    }
}

fn operation_timing(samples: &[f64]) -> OperationTimingReport {
    let total_ms = samples.iter().sum::<f64>();
    OperationTimingReport {
        calls: samples.len(),
        total_ms,
        mean_ms: if samples.is_empty() {
            0.0
        } else {
            total_ms / usize_as_f64(samples.len())
        },
        latency: (!samples.is_empty()).then(|| latency_distribution(samples)),
    }
}

fn database_report(
    pool: &PgPool,
    configured_max_connections: u32,
    pool_before: PoolSnapshot,
    request_count: usize,
    timings: &SharedEvidenceTimings,
) -> DatabaseReport {
    let observations = timings
        .lock()
        .expect("evidence timing mutex is not poisoned")
        .clone();
    let food = operation_timing(&observations.food_ms);
    let portion = operation_timing(&observations.portion_ms);
    let portion_suggestions = operation_timing(&observations.suggestions_ms);
    let total_calls = food.calls + portion.calls + portion_suggestions.calls;
    let total_observed_ms = food.total_ms + portion.total_ms + portion_suggestions.total_ms;
    let pool_during = pool_range(&observations.pool_samples);
    DatabaseReport {
        configured_max_connections,
        pool_before,
        pool_during,
        pool_after: pool_snapshot(pool),
        evidence_resolution: EvidenceResolutionReport {
            food,
            portion,
            portion_suggestions,
            total_calls,
            total_observed_ms,
            mean_observed_ms_per_request: total_observed_ms / usize_as_f64(request_count),
        },
    }
}

fn pool_range(samples: &[PoolSnapshot]) -> PoolRange {
    if samples.is_empty() {
        return PoolRange {
            observation_count: 0,
            minimum_size: 0,
            maximum_size: 0,
            minimum_idle: 0,
            maximum_idle: 0,
        };
    }
    PoolRange {
        observation_count: samples.len(),
        minimum_size: samples.iter().map(|sample| sample.size).min().unwrap_or(0),
        maximum_size: samples.iter().map(|sample| sample.size).max().unwrap_or(0),
        minimum_idle: samples.iter().map(|sample| sample.idle).min().unwrap_or(0),
        maximum_idle: samples.iter().map(|sample| sample.idle).max().unwrap_or(0),
    }
}

struct ScenarioPlan<'a> {
    scenario: &'a str,
    text: &'a str,
    request_count: usize,
    concurrency: usize,
    run_id: &'a str,
}

async fn measure_scenario(
    router: &Router,
    pool: &PgPool,
    timings: &SharedEvidenceTimings,
    fake_model: Option<&DeterministicStructuredModel>,
    plan: ScenarioPlan<'_>,
) -> (WorkloadReport, DatabaseReport) {
    let ScenarioPlan {
        scenario,
        text,
        request_count,
        concurrency,
        run_id,
    } = plan;
    let warmup_request_count = WARMUP_REQUEST_COUNT.min(request_count);
    let _warmup = run_requests(
        router,
        text,
        warmup_request_count,
        concurrency,
        run_id,
        scenario,
        "warmup",
    )
    .await;
    clear_evidence_timings(timings);
    if let Some(model) = fake_model {
        model.reset();
    }
    let pool_before = pool_snapshot(pool);
    let started = Instant::now();
    let samples = run_requests(
        router,
        text,
        request_count,
        concurrency,
        run_id,
        scenario,
        "measured",
    )
    .await;
    let elapsed = started.elapsed();
    let workload = workload_report(
        scenario,
        request_count,
        warmup_request_count,
        concurrency,
        elapsed,
        &samples,
    );
    let database = database_report(
        pool,
        API_DATABASE_POOL_SIZE,
        pool_before,
        request_count,
        timings,
    );
    (workload, database)
}

fn all_requests_succeeded(workload: &WorkloadReport) -> bool {
    !workload.status_distribution.is_empty()
        && workload.error_code_distribution.is_empty()
        && workload.status_distribution.iter().all(|(status, count)| {
            status
                .parse::<u16>()
                .is_ok_and(|status| (200..300).contains(&status) && *count > 0)
        })
        && workload.status_distribution.values().sum::<usize>() == workload.request_count
}

fn threshold_definition() -> Issue20Threshold {
    Issue20Threshold {
        comparison: "same local environment, pool size, concurrency and request count: 1 item vs 2 seeded items".to_owned(),
        minimum_p95_increase_ms: ISSUE_20_P95_ABSOLUTE_THRESHOLD_MS,
        minimum_p95_increase_fraction: ISSUE_20_P95_RELATIVE_THRESHOLD,
        minimum_p99_increase_ms: ISSUE_20_P99_ABSOLUTE_THRESHOLD_MS,
        minimum_p99_increase_fraction: ISSUE_20_P99_RELATIVE_THRESHOLD,
        minimum_added_evidence_ms_per_request: ISSUE_20_EVIDENCE_ABSOLUTE_THRESHOLD_MS,
        minimum_added_evidence_fraction_of_p95_increase:
            ISSUE_20_EVIDENCE_P95_DELTA_FRACTION,
    }
}

fn evaluate_issue20_threshold(
    threshold_revision: &str,
    one_item: &DbScenarioReport,
    multi_item: &DbScenarioReport,
) -> Issue20DecisionReport {
    let p95_increase_ms = multi_item.workload.latency.p95_ms - one_item.workload.latency.p95_ms;
    let p99_increase_ms = multi_item.workload.latency.p99_ms - one_item.workload.latency.p99_ms;
    let added_evidence_ms_per_request = multi_item
        .database
        .evidence_resolution
        .mean_observed_ms_per_request
        - one_item
            .database
            .evidence_resolution
            .mean_observed_ms_per_request;
    let added_evidence_fraction_of_p95_increase = if p95_increase_ms > 0.0 {
        added_evidence_ms_per_request / p95_increase_ms
    } else {
        0.0
    };
    let request_count = one_item.workload.request_count;
    let expected_round_trip_counts_observed = one_item.database.evidence_resolution.food.calls
        == request_count
        && one_item.database.evidence_resolution.portion.calls == request_count
        && one_item
            .database
            .evidence_resolution
            .portion_suggestions
            .calls
            == 0
        && multi_item.database.evidence_resolution.food.calls == request_count * 2
        && multi_item.database.evidence_resolution.portion.calls == request_count * 2
        && multi_item
            .database
            .evidence_resolution
            .portion_suggestions
            .calls
            == 0;
    let both_workloads_succeeded =
        all_requests_succeeded(&one_item.workload) && all_requests_succeeded(&multi_item.workload);
    let observed = Issue20Measurements {
        p95_increase_ms,
        p99_increase_ms,
        added_evidence_ms_per_request,
        added_evidence_fraction_of_p95_increase,
        one_item_evidence_calls_per_request: usize_as_f64(
            one_item.database.evidence_resolution.total_calls,
        ) / usize_as_f64(request_count),
        multi_item_evidence_calls_per_request: usize_as_f64(
            multi_item.database.evidence_resolution.total_calls,
        ) / usize_as_f64(request_count),
        both_workloads_succeeded,
        expected_round_trip_counts_observed,
    };
    let evidence_is_comparable = both_workloads_succeeded && expected_round_trip_counts_observed;
    let p95_required = ISSUE_20_P95_ABSOLUTE_THRESHOLD_MS
        .max(one_item.workload.latency.p95_ms * ISSUE_20_P95_RELATIVE_THRESHOLD);
    let p99_required = ISSUE_20_P99_ABSOLUTE_THRESHOLD_MS
        .max(one_item.workload.latency.p99_ms * ISSUE_20_P99_RELATIVE_THRESHOLD);
    let evidence_required = ISSUE_20_EVIDENCE_ABSOLUTE_THRESHOLD_MS
        .max(p95_increase_ms.max(0.0) * ISSUE_20_EVIDENCE_P95_DELTA_FRACTION);
    let triggered = evidence_is_comparable
        && p95_increase_ms >= p95_required
        && p99_increase_ms >= p99_required
        && added_evidence_ms_per_request >= evidence_required;
    let (outcome, next_action) = if !evidence_is_comparable {
        (
            "inconclusive",
            "repair_or_rerun_baseline_before_deciding_issue_20",
        )
    } else if triggered {
        (
            "threshold_triggered",
            "investigate_issue_20_without_authorizing_implementation",
        )
    } else {
        (
            "threshold_not_met",
            "close_issue_20_as_not_planned_with_this_evidence",
        )
    };
    Issue20DecisionReport {
        threshold_predeclared_at_revision: threshold_revision.to_owned(),
        outcome: outcome.to_owned(),
        next_action: next_action.to_owned(),
        triggered,
        threshold: threshold_definition(),
        observed,
    }
}

fn derived_backend_overhead(
    total: &LatencyDistribution,
    provider: &LatencyDistribution,
) -> DerivedOverheadReport {
    DerivedOverheadReport {
        p50_ms: total.p50_ms - provider.p50_ms,
        p95_ms: total.p95_ms - provider.p95_ms,
        p99_ms: total.p99_ms - provider.p99_ms,
        method: "difference between matching percentile summaries; unpaired, approximate observation, not a causal decomposition".to_owned(),
    }
}

fn validate_workload(workload: &WorkloadReport) -> Result<(), String> {
    if workload.request_count == 0 || workload.concurrency == 0 {
        return Err("request count and concurrency must be positive".to_owned());
    }
    if workload.latency.sample_count != workload.request_count {
        return Err("latency sample count must equal request count".to_owned());
    }
    if workload.status_distribution.values().sum::<usize>() != workload.request_count {
        return Err("status distribution must include every measured request".to_owned());
    }
    if !workload.elapsed_ms.is_finite()
        || workload.elapsed_ms <= 0.0
        || !workload.throughput_requests_per_second.is_finite()
        || workload.throughput_requests_per_second <= 0.0
    {
        return Err("elapsed time and throughput must be finite and positive".to_owned());
    }
    let expected_throughput = usize_as_f64(workload.request_count) * 1_000.0 / workload.elapsed_ms;
    let throughput_tolerance = expected_throughput.abs().max(1.0) * 0.000_001;
    if (workload.throughput_requests_per_second - expected_throughput).abs() > throughput_tolerance
    {
        return Err("throughput does not match request count and elapsed time".to_owned());
    }
    Ok(())
}

fn validate_report(report: &PerformanceBaselineReport) -> Result<(), String> {
    if report.schema_version != REPORT_SCHEMA_VERSION {
        return Err("unsupported performance report version".to_owned());
    }
    if !report.scope.local_ci_evidence_only
        || report.scope.production_credentials_used
        || report.scope.real_hosted_provider_called
        || report.scope.production_slo
        || report.scope.production_capacity_claim
    {
        return Err("performance evidence scope flags are unsafe".to_owned());
    }
    for scenario in &report.db_oriented_api.scenarios {
        validate_workload(&scenario.workload)?;
        if scenario.database.configured_max_connections != API_DATABASE_POOL_SIZE {
            return Err("DB pool configuration differs from the declared baseline".to_owned());
        }
    }
    validate_workload(&report.hosted_parser_analysis.workload)?;
    if report
        .hosted_parser_analysis
        .fake_profile
        .real_provider_called
        || report
            .hosted_parser_analysis
            .fake_profile
            .external_credential_required
        || report
            .hosted_parser_analysis
            .database
            .configured_max_connections
            != API_DATABASE_POOL_SIZE
    {
        return Err("hosted fake profile or DB configuration is invalid".to_owned());
    }
    if report.hosted_parser_analysis.fake_model_call_count
        != report
            .hosted_parser_analysis
            .fake_provider_latency
            .sample_count
        || report.hosted_parser_analysis.fake_model_call_count == 0
    {
        return Err("fake provider calls must match measured provider latency samples".to_owned());
    }
    if report
        .issue_20_decision
        .threshold_predeclared_at_revision
        .len()
        != 40
    {
        return Err("issue 20 threshold must reference a committed source revision".to_owned());
    }
    Ok(())
}

fn local_output_path(path: &str) -> Result<PathBuf, Box<dyn Error>> {
    let requested = PathBuf::from(path);
    let absolute = if requested.is_absolute() {
        requested
    } else {
        env::current_dir()?.join(requested)
    };
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()?;
    let parent = absolute
        .parent()
        .ok_or("baseline output path must name a file")?;
    let canonical_parent = parent.canonicalize()?;
    let file_name = absolute
        .file_name()
        .ok_or("baseline output path must name a file")?;
    let canonical_output = canonical_parent.join(file_name);
    if canonical_output.starts_with(repository_root) {
        return Err("baseline output must be outside the repository".into());
    }
    Ok(canonical_output)
}

fn verify_loopback_database_url(database_url: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(database_url)
        .map_err(|_| "database URL must be a PostgreSQL URL".to_owned())?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if !matches!(url.scheme(), "postgres" | "postgresql") || !loopback {
        return Err("performance baseline accepts only loopback PostgreSQL".to_owned());
    }
    Ok(())
}

fn command_version(command: &str, argument: &str) -> Result<String, Box<dyn Error>> {
    let output = Command::new(command).arg(argument).output()?;
    if !output.status.success() {
        return Err(format!("{command} {argument} did not complete successfully").into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn source_revision() -> Result<String, Box<dyn Error>> {
    let manifest_directory = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(manifest_directory)
        .output()?;
    if !output.status.success() {
        return Err("git could not report the baseline source revision".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn required_env_usize(name: &str, default: usize) -> Result<usize, Box<dyn Error>> {
    env::var(name).map_or_else(
        |_| Ok(default),
        |value| {
            value
                .parse::<usize>()
                .map_err(|_| format!("{name} must be a positive integer").into())
        },
    )
}

fn hosted_config() -> HostedParserConfig {
    HostedParserConfig {
        endpoint: "https://benchmark.invalid/v1/responses".to_owned(),
        api_key: "unused-local-test-placeholder".to_owned(),
        provider: "deterministic-test-fake".to_owned(),
        model: "performance-baseline-v1".to_owned(),
        timeout: Duration::from_millis(APPROVED_HOSTED_TIMEOUT_MS),
        maximum_response_bytes: APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES,
        circuit_failure_threshold: APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD,
        circuit_cooldown: Duration::from_secs(APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS),
    }
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
#[ignore = "local evidence generation; run through scripts/run-performance-baseline.ps1"]
async fn write_local_baseline_report() -> Result<(), Box<dyn Error>> {
    let database_url = env::var("TEST_DATABASE_URL")?;
    verify_loopback_database_url(&database_url)?;
    let output_path = local_output_path(&env::var("PERFORMANCE_BASELINE_OUTPUT_PATH")?)?;
    let request_count =
        required_env_usize("PERFORMANCE_BASELINE_REQUEST_COUNT", DEFAULT_REQUEST_COUNT)?;
    let concurrency = required_env_usize("PERFORMANCE_BASELINE_CONCURRENCY", DEFAULT_CONCURRENCY)?;
    if !(20..=2_000).contains(&request_count) || !(1..=32).contains(&concurrency) {
        return Err("request count must be 20..=2000 and concurrency 1..=32".into());
    }
    let pool_size =
        env::var("API_DATABASE_POOL_SIZE").map_or(Ok(API_DATABASE_POOL_SIZE), |value| {
            value
                .parse::<u32>()
                .map_err(|_| "API_DATABASE_POOL_SIZE must be an integer")
        })?;
    if pool_size != API_DATABASE_POOL_SIZE {
        return Err("baseline uses the unchanged API pool default of 8".into());
    }

    let pool = persistence_postgres::connect(&database_url, pool_size).await?;
    let catalog_release_id = active_catalog_release_id(&pool).await?;
    let postgresql_server_version: String =
        sqlx::query_scalar("SELECT current_setting('server_version')")
            .fetch_one(&pool)
            .await?;
    let source_revision = source_revision()?;
    let run_id = Uuid::new_v4().to_string();

    let single_timings = Arc::new(Mutex::new(EvidenceTimings::default()));
    let single_router = compose_router(
        pool.clone(),
        catalog_release_id,
        FixtureParser,
        "fixture-parser-0.2.0",
        "fixture/local",
        Arc::clone(&single_timings),
    );
    let (single_workload, single_database) = measure_scenario(
        &single_router,
        &pool,
        &single_timings,
        None,
        ScenarioPlan {
            scenario: "fixture_api_create_one_seeded_item",
            text: DB_SINGLE_ITEM_TEXT,
            request_count,
            concurrency,
            run_id: &run_id,
        },
    )
    .await;
    let single_scenario = DbScenarioReport {
        item_count: 1,
        workload: single_workload,
        database: single_database,
    };
    drop(single_router);

    let multi_timings = Arc::new(Mutex::new(EvidenceTimings::default()));
    let multi_router = compose_router(
        pool.clone(),
        catalog_release_id,
        FixtureParser,
        "fixture-parser-0.2.0",
        "fixture/local",
        Arc::clone(&multi_timings),
    );
    let (multi_workload, multi_database) = measure_scenario(
        &multi_router,
        &pool,
        &multi_timings,
        None,
        ScenarioPlan {
            scenario: "fixture_api_create_two_seeded_items",
            text: DB_MULTI_ITEM_TEXT,
            request_count,
            concurrency,
            run_id: &run_id,
        },
    )
    .await;
    let multi_scenario = DbScenarioReport {
        item_count: 2,
        workload: multi_workload,
        database: multi_database,
    };
    drop(multi_router);

    let hosted_timings = Arc::new(Mutex::new(EvidenceTimings::default()));
    let profile = FakeProfile {
        fixed_delay: Duration::from_millis(FAKE_PROVIDER_DELAY_MS),
        error_every_nth_call: Some(FAKE_TRANSIENT_ERROR_EVERY),
        failure_kind: Some(FakeFailureKind::Transient),
    };
    let fake_model = Arc::new(DeterministicStructuredModel::new(profile));
    let hosted_parser = HostedMealParser::new(hosted_config(), fake_model.clone())?;
    let hosted_router = compose_router(
        pool.clone(),
        catalog_release_id,
        hosted_parser,
        HOSTED_PROMPT_VERSION,
        "deterministic-test-fake/performance-baseline-v1",
        Arc::clone(&hosted_timings),
    );
    let (hosted_workload, hosted_database) = measure_scenario(
        &hosted_router,
        &pool,
        &hosted_timings,
        Some(&fake_model),
        ScenarioPlan {
            scenario: "hosted_parser_in_process_deterministic_fake",
            text: HOSTED_FAKE_TEXT,
            request_count,
            concurrency,
            run_id: &run_id,
        },
    )
    .await;
    let provider_distribution = latency_distribution(&fake_model.observed_latency_ms());
    let hosted_report = HostedParserReport {
        request_path:
            "in-process Axum router with HostedMealParser and deterministic StructuredModel fake"
                .to_owned(),
        parser: "HostedMealParser".to_owned(),
        derived_backend_overhead_observation: derived_backend_overhead(
            &hosted_workload.latency,
            &provider_distribution,
        ),
        workload: hosted_workload,
        database: hosted_database,
        fake_profile: FakeProfileReport {
            fake_model: "deterministic-test-fake/performance-baseline-v1".to_owned(),
            fixed_delay_ms: FAKE_PROVIDER_DELAY_MS,
            transient_error_every_nth_call: FAKE_TRANSIENT_ERROR_EVERY,
            transient_error_code: "benchmark_transient".to_owned(),
            external_credential_required: false,
            real_provider_called: false,
        },
        fake_model_call_count: fake_model.call_count(),
        fake_provider_latency: provider_distribution,
    };

    let revision = source_revision;
    let decision = evaluate_issue20_threshold(&revision, &single_scenario, &multi_scenario);
    let report = PerformanceBaselineReport {
        schema_version: REPORT_SCHEMA_VERSION.to_owned(),
        scope: ReportScope {
            local_ci_evidence_only: true,
            production_credentials_used: false,
            real_hosted_provider_called: false,
            production_slo: false,
            production_capacity_claim: false,
        },
        percentile_definition: "nearest-rank: sort ascending, rank = ceil(percent * sample_count), one-based; p50/p95/p99 select that rank".to_owned(),
        environment: EnvironmentMetadata {
            source_revision: revision,
            operating_system: env::consts::OS.to_owned(),
            architecture: env::consts::ARCH.to_owned(),
            rustc_version: command_version("rustc", "--version")?,
            build_profile: "test-debug".to_owned(),
            logical_cpu_count: std::thread::available_parallelism()
                .ok()
                .map(std::num::NonZeroUsize::get),
            postgresql_server_version,
            dataset: env::var("PERFORMANCE_BASELINE_DATASET")
                .unwrap_or_else(|_| "foundation-fixture-seed".to_owned()),
        },
        db_oriented_api: DbOrientedReport {
            request_path: "in-process Axum router; no TCP listener or hosted parser".to_owned(),
            parser: "FixtureParser".to_owned(),
            scenarios: vec![single_scenario, multi_scenario],
        },
        hosted_parser_analysis: hosted_report,
        issue_20_decision: decision,
    };
    validate_report(&report).map_err(|error| format!("invalid performance report: {error}"))?;
    fs::write(output_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_values_use_the_documented_nearest_rank_definition() {
        let values = [100.0, 1.0, 4.0, 2.0, 3.0];
        let distribution = latency_distribution(&values);
        assert_eq!(distribution.sample_count, 5);
        assert!((distribution.min_ms - 1.0).abs() < f64::EPSILON);
        assert!((distribution.max_ms - 100.0).abs() < f64::EPSILON);
        assert!((distribution.p50_ms - 3.0).abs() < f64::EPSILON);
        assert!((distribution.p95_ms - 100.0).abs() < f64::EPSILON);
        assert!((distribution.p99_ms - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn issue_20_threshold_requires_latency_growth_and_measured_evidence_cost() {
        let base = make_decision_scenario("one", 1, &[20.0; 100], 2, 4.0);
        let triggered = make_decision_scenario("multi", 2, &[80.0; 100], 4, 40.0);
        let result = evaluate_issue20_threshold(&"1".repeat(40), &base, &triggered);
        assert_eq!(result.outcome, "threshold_triggered");
        assert!(result.triggered);

        let below_threshold = make_decision_scenario("multi", 2, &[30.0; 100], 4, 8.0);
        let result = evaluate_issue20_threshold(&"1".repeat(40), &base, &below_threshold);
        assert_eq!(result.outcome, "threshold_not_met");
        assert!(!result.triggered);
    }

    #[test]
    fn issue_20_threshold_is_inconclusive_when_workloads_or_query_counts_are_invalid() {
        let base = make_decision_scenario("one", 1, &[20.0; 20], 2, 4.0);
        let mut multi = make_decision_scenario("multi", 2, &[80.0; 20], 4, 30.0);
        multi
            .workload
            .status_distribution
            .insert("201".to_owned(), 19);
        multi
            .workload
            .status_distribution
            .insert("503".to_owned(), 1);
        multi.workload.request_count = 20;
        let result = evaluate_issue20_threshold(&"1".repeat(40), &base, &multi);
        assert_eq!(result.outcome, "inconclusive");
    }

    #[tokio::test]
    async fn fake_profile_applies_fixed_delay_and_deterministic_transient_cadence() {
        let fake = DeterministicStructuredModel::new(FakeProfile {
            fixed_delay: Duration::from_millis(2),
            error_every_nth_call: Some(2),
            failure_kind: Some(FakeFailureKind::Transient),
        });
        let request = fake_generation_request();
        let first = Instant::now();
        assert!(fake.generate(&request, 16_384).await.is_ok());
        assert!(first.elapsed() >= Duration::from_millis(2));
        let error = fake
            .generate(&request, 16_384)
            .await
            .expect_err("every second model call fails by profile");
        assert_eq!(
            error.classification,
            StructuredModelErrorClassification::Transient
        );
        assert_eq!(error.code(), "benchmark_transient");
        assert!(fake.generate(&request, 16_384).await.is_ok());
        assert_eq!(fake.call_count(), 3);
        assert_eq!(fake.observed_latency_ms().len(), 3);
    }

    #[tokio::test]
    async fn fake_profile_can_report_a_deterministic_permanent_error() {
        let fake = DeterministicStructuredModel::new(FakeProfile {
            fixed_delay: Duration::ZERO,
            error_every_nth_call: Some(1),
            failure_kind: Some(FakeFailureKind::Permanent),
        });
        let error = fake
            .generate(&fake_generation_request(), 16_384)
            .await
            .expect_err("every call is a permanent failure by profile");
        assert_eq!(
            error.classification,
            StructuredModelErrorClassification::Permanent
        );
        assert_eq!(error.code(), "benchmark_permanent");
    }

    #[test]
    fn report_matches_versioned_schema_and_contains_no_run_payload() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../schemas/performance-baseline-0.1.0.json"
        ))
        .expect("performance baseline schema is valid JSON");
        assert_eq!(schema["$id"], "performance-baseline-0.1.0");
        let required = schema["required"].as_array().expect("schema required list");
        for key in [
            "schema_version",
            "scope",
            "environment",
            "db_oriented_api",
            "hosted_parser_analysis",
            "issue_20_decision",
        ] {
            assert!(required.iter().any(|value| value == key));
        }
        assert!(schema["$defs"]["scope"]["properties"]["real_hosted_provider_called"].is_object());
        assert!(schema["$defs"]["dbOrientedApi"]["properties"]["scenarios"].is_object());
        assert!(schema["$defs"]["hostedParserAnalysis"]["properties"]["fake_profile"].is_object());
    }

    fn fake_generation_request() -> StructuredGenerationRequest {
        StructuredGenerationRequest {
            provider: ProviderIdentity::new("deterministic-test-fake"),
            model: ModelIdentity::new("performance-baseline-v1"),
            system_instruction: "test-only deterministic generation".to_owned(),
            untrusted_input: UntrustedInput::new("locale: vi-VN\nmeal: 2 quả trứng gà luộc"),
            schema: StrictJsonSchema::new(json!({"type": "object"})),
        }
    }

    fn make_decision_scenario(
        scenario: &str,
        items: usize,
        latencies: &[f64],
        calls_per_request: usize,
        evidence_ms_per_request: f64,
    ) -> DbScenarioReport {
        let request_count = latencies.len();
        let status_distribution = BTreeMap::from([("201".to_owned(), request_count)]);
        let workload = WorkloadReport {
            scenario: scenario.to_owned(),
            operation: "POST /v1/nutrition/analyses".to_owned(),
            request_count,
            warmup_request_count: 10,
            concurrency: 4,
            elapsed_ms: usize_as_f64(request_count),
            throughput_requests_per_second: usize_as_f64(request_count),
            latency: latency_distribution(latencies),
            status_distribution,
            error_code_distribution: BTreeMap::new(),
        };
        let operation = |calls| {
            let mean_ms = if calls == 0 {
                0.0
            } else {
                evidence_ms_per_request / usize_as_f64(calls_per_request)
            };
            OperationTimingReport {
                calls,
                total_ms: mean_ms * usize_as_f64(calls),
                mean_ms,
                latency: (calls > 0).then(|| latency_distribution(&vec![mean_ms; calls])),
            }
        };
        DbScenarioReport {
            item_count: items,
            workload,
            database: DatabaseReport {
                configured_max_connections: API_DATABASE_POOL_SIZE,
                pool_before: PoolSnapshot { size: 1, idle: 1 },
                pool_during: PoolRange {
                    observation_count: 1,
                    minimum_size: 1,
                    maximum_size: 1,
                    minimum_idle: 0,
                    maximum_idle: 1,
                },
                pool_after: PoolSnapshot { size: 1, idle: 1 },
                evidence_resolution: EvidenceResolutionReport {
                    food: operation(request_count * items),
                    portion: operation(request_count * items),
                    portion_suggestions: operation(0),
                    total_calls: request_count * calls_per_request,
                    total_observed_ms: evidence_ms_per_request * usize_as_f64(request_count),
                    mean_observed_ms_per_request: evidence_ms_per_request,
                },
            },
        }
    }
}
