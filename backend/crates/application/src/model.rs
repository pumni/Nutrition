use domain::{
    AnalysisId, AnalysisRevisionId, CalculationResult, CatalogReleaseId, ClarificationQuestionId,
    CompositionProfileId, CompositionSnapshot, EvidenceQuality, FoodId, MassEstimate,
    MassResolutionMethod, NutrientCode, PortionObservationId, UserId,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMode {
    Fast,
    #[default]
    Balanced,
    Precise,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalysisRequest {
    pub text: String,
    pub locale: String,
    #[serde(default)]
    pub mode: AnalysisMode,
    #[serde(skip)]
    pub idempotency: Option<IdempotencyContext>,
    #[serde(skip)]
    pub owner_id: Option<UserId>,
}

#[derive(Clone, Debug)]
pub struct ParseRequest {
    pub text: String,
    pub locale: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParsedMealDocument {
    pub language: String,
    pub items: Vec<ParsedMealItem>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParsedMealItem {
    pub source_text: String,
    pub food_phrase: String,
    pub quantity: Option<Decimal>,
    pub unit_phrase: Option<String>,
    pub modifiers: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ParserInvocationRecord {
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
    pub schema_version: String,
    pub latency_ms: i64,
    pub retry_count: i32,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub output_sha256: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ResolvedFoodEvidence {
    pub food_id: FoodId,
    pub food_name: String,
    pub composition: CompositionSnapshot,
    pub quality: EvidenceQuality,
}

#[derive(Clone, Debug)]
pub struct ResolvedPortionEvidence {
    pub mass: MassEstimate,
    pub quality: EvidenceQuality,
    pub assumptions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BehaviorVersions {
    pub application_version: String,
    pub parser_schema_version: String,
    pub prompt_version: String,
    pub model_provider_version: String,
    pub normalization_version: String,
    pub resolution_policy_version: String,
    pub portion_policy_version: String,
    pub composition_policy_version: String,
    pub clarification_policy_version: String,
    pub correction_policy_version: String,
    pub calculation_engine_version: String,
    pub catalog_release_id: CatalogReleaseId,
}

impl Default for BehaviorVersions {
    fn default() -> Self {
        Self {
            application_version: env!("CARGO_PKG_VERSION").to_owned(),
            parser_schema_version: "parsed-meal-0.1.0".to_owned(),
            prompt_version: "fixture-parser-0.2.0".to_owned(),
            model_provider_version: "fixture/local".to_owned(),
            normalization_version: "normalize-0.1.0".to_owned(),
            resolution_policy_version: "resolve-exact-0.1.0".to_owned(),
            portion_policy_version: "portion-contextual-0.2.0".to_owned(),
            composition_policy_version: "composition-direct-0.1.0".to_owned(),
            clarification_policy_version: "clarification-portion-0.1.0".to_owned(),
            correction_policy_version: "correction-portion-0.1.0".to_owned(),
            calculation_engine_version: domain::CALCULATION_ENGINE_VERSION.to_owned(),
            catalog_release_id: CatalogReleaseId::from_u128(
                0x0198_f100_0000_7000_8000_0000_0000_0002,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Completed,
    NeedsClarification,
    InsufficientEvidence,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalysisItemSnapshot {
    pub source_text: String,
    pub food_id: FoodId,
    pub food_name: String,
    #[serde(default)]
    pub quantity: Option<Decimal>,
    #[serde(default)]
    pub unit_phrase: Option<String>,
    pub profile_id: CompositionProfileId,
    pub portion_observation_id: Option<PortionObservationId>,
    pub estimated_mass_g: Decimal,
    pub lower_mass_g: Option<Decimal>,
    pub upper_mass_g: Option<Decimal>,
    pub mass_resolution_method: MassResolutionMethod,
    pub evidence_quality: EvidenceQuality,
    pub assumptions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalysisSnapshot {
    pub analysis_id: AnalysisId,
    pub revision_id: AnalysisRevisionId,
    pub revision_number: u32,
    pub status: AnalysisStatus,
    pub locale: String,
    pub versions: BehaviorVersions,
    pub items: Vec<AnalysisItemSnapshot>,
    pub requested_nutrients: Vec<NutrientCode>,
    pub calculation: CalculationResult,
    pub is_estimate: bool,
    #[serde(skip)]
    pub idempotency: Option<IdempotencyContext>,
    #[serde(skip)]
    pub owner_id: Option<UserId>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClarificationOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClarificationQuestion {
    pub id: ClarificationQuestionId,
    pub dimension: String,
    pub prompt: String,
    pub options: Vec<ClarificationOption>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClarificationContext {
    pub item: ParsedMealItem,
    pub food_id: FoodId,
    pub food_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClarificationAnalysis {
    pub analysis_id: AnalysisId,
    pub revision_id: AnalysisRevisionId,
    pub revision_number: u32,
    pub status: AnalysisStatus,
    pub locale: String,
    pub versions: BehaviorVersions,
    pub question: ClarificationQuestion,
    #[serde(skip_serializing)]
    pub context: ClarificationContext,
    #[serde(skip)]
    pub idempotency: Option<IdempotencyContext>,
    #[serde(skip)]
    pub owner_id: Option<UserId>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum AnalysisOutcome {
    Completed(AnalysisSnapshot),
    NeedsClarification(ClarificationAnalysis),
}

impl AnalysisOutcome {
    #[must_use]
    pub const fn analysis_id(&self) -> AnalysisId {
        match self {
            Self::Completed(snapshot) => snapshot.analysis_id,
            Self::NeedsClarification(clarification) => clarification.analysis_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClarificationAnswerRequest {
    pub expected_revision_id: AnalysisRevisionId,
    pub question_id: ClarificationQuestionId,
    pub option_id: String,
    pub mass_g: Option<Decimal>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PortionCorrection {
    pub item_index: usize,
    pub quantity: Decimal,
    pub unit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CorrectionRequest {
    pub base_revision_id: AnalysisRevisionId,
    pub item_corrections: Vec<PortionCorrection>,
    #[serde(skip)]
    pub idempotency: Option<IdempotencyContext>,
}

#[derive(Clone, Debug)]
pub struct PortionSuggestion {
    pub unit: String,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct IdempotencyContext {
    pub scope_key: String,
    pub key: String,
    pub request_hash: String,
}
