use domain::{
    AnalysisId, AnalysisRevisionId, CalculationResult, CompositionSnapshot, EvidenceQuality,
    FoodId, MassEstimate, NutrientCode,
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
}

#[derive(Clone, Debug)]
pub struct ParseRequest {
    pub text: String,
    pub locale: String,
}

#[derive(Clone, Debug)]
pub struct ParsedMealDocument {
    pub language: String,
    pub items: Vec<ParsedMealItem>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ParsedMealItem {
    pub source_text: String,
    pub food_phrase: String,
    pub quantity: Option<Decimal>,
    pub unit_phrase: Option<String>,
    pub modifiers: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ResolvedEvidence {
    pub food_id: FoodId,
    pub food_name: String,
    pub mass: MassEstimate,
    pub composition: CompositionSnapshot,
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
    pub calculation_engine_version: String,
    pub catalog_release_id: String,
}

impl Default for BehaviorVersions {
    fn default() -> Self {
        Self {
            application_version: env!("CARGO_PKG_VERSION").to_owned(),
            parser_schema_version: "parsed-meal-0.1.0".to_owned(),
            prompt_version: "fixture-parser-0.1.0".to_owned(),
            model_provider_version: "fixture/local".to_owned(),
            normalization_version: "normalize-0.1.0".to_owned(),
            resolution_policy_version: "resolve-exact-0.1.0".to_owned(),
            portion_policy_version: "portion-explicit-0.1.0".to_owned(),
            composition_policy_version: "composition-direct-0.1.0".to_owned(),
            calculation_engine_version: domain::CALCULATION_ENGINE_VERSION.to_owned(),
            catalog_release_id: "catalog-foundation-0.1.0".to_owned(),
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
    pub estimated_mass_g: Decimal,
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
}
