use adapters::{FixtureParser, InMemoryAnalysisRepository};
use application::{
    AnalysisOutcome, AnalysisRepository, AnalysisRequest, AnalysisSnapshot, AnalysisStatus,
    AnalyzeMeal, ApplicationError, BehaviorVersions, ClarificationAnalysis,
    ClarificationAnswerRequest, CorrectionRequest, FoodEvidenceProvider, MealAnalysisService,
    ParsedMealItem, PortionEvidenceProvider, PortionSuggestion, ResolvedFoodEvidence,
    ResolvedPortionEvidence, ensure_modifiers_represented, normalize_vi_search_key,
};
use async_trait::async_trait;
use domain::{
    CatalogReleaseId, CompositionProfileId, CompositionSnapshot, CompositionValue, EvidenceQuality,
    FoodId, MassEstimate, MassResolutionMethod, NutrientCode, NutrientUnit, PortionObservationId,
    ValueStatus,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

const COMPOSITION_SHA256: &str = "f1d0434af02bc18f128e0f4910464971d7ac34690f4e5cf14bce85e26de5410c";
const REVIEWED_MAPPING_SHA256: &str =
    "c7ab626ed66c01a84c6b00d7e312a3e061eef4af2ec4dcfed950e6b398307324";
const SOURCE_RECORD_SHA256: &str =
    "634cf6fabbcc9ccb76f513ef274a9dcc3d92f495c78792cf8221170d98a102cc";
const REQUESTED_NUTRIENTS: [&str; 4] = ["energy_kcal", "protein_g", "carbohydrate_g", "fat_g"];

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Deserialize)]
struct ReviewedCompositionArtifact {
    schema_version: String,
    policy_version: String,
    normalized_vietnamese_target: String,
    reviewed_mapping_sha256: String,
    source_code: String,
    source_release: String,
    fndds_food_code: String,
    fdc_id: u64,
    source_record_sha256: String,
    basis_amount: Decimal,
    basis_unit: String,
    edible_basis: bool,
    nutrients: Vec<ReviewedNutrient>,
    evaluation_eligible: bool,
    catalog_staging_authorized: bool,
    production_eligible: bool,
    activation_authorized: bool,
    portion_evidence_authorized: bool,
    recipe_evidence_authorized: bool,
}

#[derive(Debug, Deserialize)]
struct ReviewedNutrient {
    target_code: String,
    fndds_nutrient_code: String,
    source_nutrient_id: u64,
    source_unit: String,
    source_amount: String,
    canonical_unit: String,
}

#[derive(Clone)]
struct ReviewedRiceEvidence {
    composition: CompositionSnapshot,
    source_amounts: BTreeMap<String, Decimal>,
}

impl ReviewedRiceEvidence {
    fn load_committed() -> Result<Self, String> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let artifact = fs::read(root.join(
            "data-factory/docs/reviews/vietnamese-com-trang-reviewed-composition-0.1.0.json",
        ))
        .map_err(|error| format!("failed to read reviewed composition artifact: {error}"))?;
        let schema =
            fs::read(root.join("data-factory/schemas/reviewed-food-composition-0.1.0.schema.json"))
                .map_err(|error| format!("failed to read reviewed composition schema: {error}"))?;
        let mapping = fs::read(root.join(
            "data-factory/docs/reviews/vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json",
        ))
        .map_err(|error| format!("failed to read reviewed mapping artifact: {error}"))?;
        Self::from_bytes(&artifact, &schema, &mapping)
    }

    fn from_bytes(
        artifact_bytes: &[u8],
        schema_bytes: &[u8],
        mapping_bytes: &[u8],
    ) -> Result<Self, String> {
        let artifact = parse_pinned_artifact(artifact_bytes, schema_bytes)?;
        validate_artifact_metadata(&artifact)?;

        let actual_mapping_hash = hex::encode(Sha256::digest(mapping_bytes));
        if actual_mapping_hash != REVIEWED_MAPPING_SHA256
            || artifact.reviewed_mapping_sha256 != actual_mapping_hash
        {
            return Err("reviewed mapping artifact SHA-256 changed".to_owned());
        }

        let mut source_amounts = BTreeMap::new();
        let mut values = Vec::with_capacity(artifact.nutrients.len());
        let mut targets = BTreeSet::new();
        for nutrient in artifact.nutrients {
            let (unit, expected_unit, expected_fndds_code, expected_source_id) =
                match nutrient.target_code.as_str() {
                    "energy_kcal" => (NutrientUnit::Kilocalorie, "kcal", "208", [1008, 2047]),
                    "protein_g" => (NutrientUnit::Gram, "g", "203", [1003, 1003]),
                    "carbohydrate_g" => (NutrientUnit::Gram, "g", "205", [1005, 1005]),
                    "fat_g" => (NutrientUnit::Gram, "g", "204", [1004, 1004]),
                    _ => {
                        return Err(
                            "reviewed composition contains an unknown nutrient target".to_owned()
                        );
                    }
                };
            if nutrient.canonical_unit != expected_unit
                || nutrient.source_unit != expected_unit
                || nutrient.fndds_nutrient_code != expected_fndds_code
                || !expected_source_id.contains(&nutrient.source_nutrient_id)
            {
                return Err("reviewed composition nutrient identity or unit changed".to_owned());
            }
            if !targets.insert(nutrient.target_code.clone()) {
                return Err("reviewed composition contains a duplicate nutrient target".to_owned());
            }
            let amount = Decimal::from_str(&nutrient.source_amount)
                .map_err(|error| format!("reviewed nutrient amount is not a decimal: {error}"))?;
            let nutrient_code = NutrientCode::new(nutrient.target_code.clone())
                .map_err(|error| format!("reviewed nutrient code is invalid: {error}"))?;
            source_amounts.insert(nutrient.target_code, amount);
            values.push(CompositionValue {
                nutrient: nutrient_code,
                amount: Some(amount),
                lower_amount: None,
                upper_amount: None,
                unit,
                status: ValueStatus::Compiled,
            });
        }
        if targets != REQUESTED_NUTRIENTS.into_iter().map(str::to_owned).collect() {
            return Err(
                "reviewed composition does not contain exactly the four required nutrients"
                    .to_owned(),
            );
        }

        Ok(Self {
            composition: CompositionSnapshot {
                // This profile ID is generated for this test run only; it is not a catalog ID.
                profile_id: CompositionProfileId::new(),
                basis_g: artifact.basis_amount,
                quality: EvidenceQuality::U,
                values,
            },
            source_amounts,
        })
    }
}

fn parse_pinned_artifact(
    artifact_bytes: &[u8],
    schema_bytes: &[u8],
) -> Result<ReviewedCompositionArtifact, String> {
    let artifact_hash = hex::encode(Sha256::digest(artifact_bytes));
    if artifact_hash != COMPOSITION_SHA256 {
        return Err("reviewed composition artifact SHA-256 changed".to_owned());
    }
    let schema: Value = serde_json::from_slice(schema_bytes)
        .map_err(|error| format!("reviewed composition schema is invalid JSON: {error}"))?;
    let artifact_value: Value = serde_json::from_slice(artifact_bytes)
        .map_err(|error| format!("reviewed composition artifact is invalid JSON: {error}"))?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|error| format!("reviewed composition schema is invalid: {error}"))?;
    validator
        .validate(&artifact_value)
        .map_err(|error| format!("reviewed composition artifact violates schema: {error}"))?;
    serde_json::from_value(artifact_value)
        .map_err(|error| format!("reviewed composition artifact has invalid fields: {error}"))
}

fn validate_artifact_metadata(artifact: &ReviewedCompositionArtifact) -> Result<(), String> {
    if artifact.schema_version != "reviewed-food-composition-0.1.0"
        || artifact.policy_version != "reviewed-composition-evaluation-0.1.0"
        || artifact.normalized_vietnamese_target != "cơm trắng"
        || artifact.source_code != "usda_fndds"
        || artifact.source_release != "2021-2023 / October 2024"
        || artifact.fndds_food_code != "56205008"
        || artifact.fdc_id != 2_708_408
        || artifact.source_record_sha256 != SOURCE_RECORD_SHA256
        || artifact.basis_amount != Decimal::from(100)
        || artifact.basis_unit != "g"
        || !artifact.edible_basis
        || !artifact.evaluation_eligible
        || artifact.catalog_staging_authorized
        || artifact.production_eligible
        || artifact.activation_authorized
        || artifact.portion_evidence_authorized
        || artifact.recipe_evidence_authorized
    {
        return Err(
            "reviewed composition identity, basis, or authorization boundary changed".to_owned(),
        );
    }
    Ok(())
}

#[derive(Clone)]
struct ReviewedRiceFoodProvider {
    evidence: ReviewedRiceEvidence,
    food_id: FoodId,
}

impl ReviewedRiceFoodProvider {
    fn new(evidence: ReviewedRiceEvidence) -> Self {
        Self {
            evidence,
            // Opaque ID required only by the existing application/domain interfaces in this test.
            food_id: FoodId::new(),
        }
    }
}

#[async_trait]
impl FoodEvidenceProvider for ReviewedRiceFoodProvider {
    async fn resolve_food(
        &self,
        _locale: &str,
        item: &ParsedMealItem,
    ) -> Result<ResolvedFoodEvidence, ApplicationError> {
        ensure_modifiers_represented(&item.food_phrase, &item.modifiers)?;
        if normalize_vi_search_key(&item.food_phrase) != "cơm trắng" {
            return Err(ApplicationError::InsufficientEvidence(
                "food identity is not the exact reviewed generic target".to_owned(),
            ));
        }
        Ok(ResolvedFoodEvidence {
            food_id: self.food_id,
            food_name: "Cơm trắng".to_owned(),
            composition: self.evidence.composition.clone(),
            quality: EvidenceQuality::U,
        })
    }
}

#[derive(Clone, Copy)]
struct EvaluationPortionProvider;

#[async_trait]
impl PortionEvidenceProvider for EvaluationPortionProvider {
    async fn resolve_portion(
        &self,
        _locale: &str,
        item: &ParsedMealItem,
        _food_id: FoodId,
    ) -> Result<ResolvedPortionEvidence, ApplicationError> {
        let quantity = item.quantity.ok_or_else(|| {
            ApplicationError::InsufficientEvidence("explicit quantity is required".to_owned())
        })?;
        if quantity <= Decimal::ZERO {
            return Err(ApplicationError::InsufficientEvidence(
                "quantity must be positive".to_owned(),
            ));
        }
        if item.unit_phrase.as_deref() != Some("g") {
            return Err(ApplicationError::InsufficientEvidence(
                "no reviewed contextual portion evidence".to_owned(),
            ));
        }
        Ok(ResolvedPortionEvidence {
            mass: MassEstimate {
                central_g: quantity,
                lower_g: None,
                upper_g: None,
                evidence_id: None::<PortionObservationId>,
                method: MassResolutionMethod::ExplicitMass,
            },
            quality: EvidenceQuality::U,
            assumptions: Vec::new(),
        })
    }

    async fn suggestions(
        &self,
        _locale: &str,
        _food_id: FoodId,
    ) -> Result<Vec<PortionSuggestion>, ApplicationError> {
        Ok(Vec::new())
    }
}

#[derive(Clone)]
struct SharedInMemoryRepository(Arc<InMemoryAnalysisRepository>);

#[async_trait]
impl AnalysisRepository for SharedInMemoryRepository {
    async fn save(&self, snapshot: &AnalysisSnapshot) -> Result<(), ApplicationError> {
        self.0.save(snapshot).await
    }

    async fn save_clarification(
        &self,
        clarification: &ClarificationAnalysis,
    ) -> Result<(), ApplicationError> {
        self.0.save_clarification(clarification).await
    }

    async fn find_open_clarification(
        &self,
        analysis_id: domain::AnalysisId,
    ) -> Result<Option<ClarificationAnalysis>, ApplicationError> {
        self.0.find_open_clarification(analysis_id).await
    }

    async fn append_clarification_answer(
        &self,
        answer: &ClarificationAnswerRequest,
        snapshot: &AnalysisSnapshot,
    ) -> Result<(), ApplicationError> {
        self.0.append_clarification_answer(answer, snapshot).await
    }

    async fn append_correction(
        &self,
        request: &CorrectionRequest,
        snapshot: &AnalysisSnapshot,
    ) -> Result<(), ApplicationError> {
        self.0.append_correction(request, snapshot).await
    }
}

fn committed_input_bytes() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    (
        fs::read(
            root.join("data-factory/docs/reviews/vietnamese-com-trang-reviewed-composition-0.1.0.json"),
        )
        .expect("reviewed composition artifact must be readable"),
        fs::read(root.join("data-factory/schemas/reviewed-food-composition-0.1.0.schema.json"))
            .expect("reviewed composition schema must be readable"),
        fs::read(root.join(
            "data-factory/docs/reviews/vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json",
        ))
        .expect("reviewed mapping artifact must be readable"),
    )
}

fn evaluation_versions() -> BehaviorVersions {
    // BehaviorVersions requires a release ID; keep this generated ID local to the evaluation.
    BehaviorVersions {
        composition_policy_version: "reviewed-composition-evaluation-0.1.0".to_owned(),
        catalog_release_id: CatalogReleaseId::new(),
        ..BehaviorVersions::default()
    }
}

fn requested_nutrients() -> Vec<NutrientCode> {
    REQUESTED_NUTRIENTS
        .into_iter()
        .map(|code| NutrientCode::new(code).expect("requested nutrient code is valid"))
        .collect()
}

#[test]
fn reviewed_composition_and_mapping_are_hash_pinned_and_schema_valid() {
    let (artifact, schema, mapping) = committed_input_bytes();
    let evidence = ReviewedRiceEvidence::from_bytes(&artifact, &schema, &mapping)
        .expect("reviewed evidence must match its pins and schema");
    assert_eq!(evidence.source_amounts.len(), 4);
    assert_eq!(evidence.composition.basis_g, Decimal::from(100));
    assert_eq!(evidence.composition.quality, EvidenceQuality::U);
    assert!(
        evidence
            .composition
            .values
            .iter()
            .all(|value| value.status == ValueStatus::Compiled)
    );
}

#[test]
fn artifact_hash_mapping_hash_identity_and_authorization_drift_fail_closed() {
    let (artifact, schema, mapping) = committed_input_bytes();
    let mut wrong_artifact_hash = artifact.clone();
    wrong_artifact_hash.push(b' ');
    assert!(ReviewedRiceEvidence::from_bytes(&wrong_artifact_hash, &schema, &mapping).is_err());

    let mut wrong_mapping = mapping.clone();
    wrong_mapping.push(b' ');
    assert!(ReviewedRiceEvidence::from_bytes(&artifact, &schema, &wrong_mapping).is_err());

    for (field, replacement) in [
        ("catalog_staging_authorized", Value::Bool(true)),
        ("production_eligible", Value::Bool(true)),
        ("activation_authorized", Value::Bool(true)),
        ("portion_evidence_authorized", Value::Bool(true)),
        ("recipe_evidence_authorized", Value::Bool(true)),
        (
            "normalized_vietnamese_target",
            Value::String("cơm trắng jasmine".to_owned()),
        ),
        ("source_code", Value::String("other_source".to_owned())),
    ] {
        let mut changed: Value = serde_json::from_slice(&artifact).expect("artifact JSON is valid");
        changed[field] = replacement;
        let changed_bytes = serde_json::to_vec(&changed).expect("mutated artifact serializes");
        assert!(
            ReviewedRiceEvidence::from_bytes(&changed_bytes, &schema, &mapping).is_err(),
            "mutation of {field} must fail closed"
        );
    }
}

#[tokio::test]
async fn explicit_100g_rice_uses_reviewed_composition_through_analysis_service() {
    let evidence = ReviewedRiceEvidence::load_committed().expect("reviewed evidence must load");
    let expected_amounts = evidence.source_amounts.clone();
    let repository = Arc::new(InMemoryAnalysisRepository::default());
    let service = MealAnalysisService::new(
        FixtureParser,
        ReviewedRiceFoodProvider::new(evidence),
        EvaluationPortionProvider,
        SharedInMemoryRepository(Arc::clone(&repository)),
        evaluation_versions(),
        requested_nutrients(),
    );

    let outcome = service
        .execute(AnalysisRequest {
            text: "100 g cơm trắng".to_owned(),
            locale: "vi-VN".to_owned(),
            idempotency: None,
            owner_id: None,
        })
        .await
        .expect("explicit grams with reviewed composition must calculate");
    let AnalysisOutcome::Completed(snapshot) = outcome else {
        panic!("explicit 100 g rice must complete");
    };

    assert_eq!(snapshot.status, AnalysisStatus::Completed);
    assert_eq!(snapshot.items.len(), 1);
    let item = &snapshot.items[0];
    assert_eq!(item.source_text, "100 g cơm trắng");
    assert_eq!(item.food_name, "Cơm trắng");
    assert_eq!(item.estimated_mass_g, Decimal::from(100));
    assert_eq!(
        item.mass_resolution_method,
        MassResolutionMethod::ExplicitMass
    );
    assert_eq!(item.portion_observation_id, None);
    assert_eq!(item.lower_mass_g, None);
    assert_eq!(item.upper_mass_g, None);
    assert_eq!(
        snapshot.calculation.engine_version,
        domain::CALCULATION_ENGINE_VERSION
    );
    assert_eq!(
        snapshot.versions.resolution_policy_version,
        "resolve-exact-specificity-0.2.0"
    );
    assert_eq!(
        snapshot.versions.composition_policy_version,
        "reviewed-composition-evaluation-0.1.0"
    );
    assert_eq!(
        snapshot.versions.clarification_policy_version,
        "clarification-portion-0.2.0"
    );

    let totals = &snapshot.calculation.totals;
    assert_eq!(totals.len(), 4);
    for total in totals {
        let target = total.nutrient.as_str();
        assert_eq!(total.amount, expected_amounts.get(target).copied());
        assert_eq!(total.completeness_ratio, Decimal::ONE);
        assert_eq!(total.lower_amount, None);
        assert_eq!(total.upper_amount, None);
    }
    assert_eq!(snapshot.calculation.items.len(), 1);
    let calculated_item = &snapshot.calculation.items[0];
    assert_eq!(calculated_item.mass_g, Decimal::from(100));
    for nutrient in &calculated_item.nutrients {
        let operation = nutrient
            .operation
            .as_ref()
            .expect("reviewed value has calculation operation");
        assert_eq!(operation.basis_g, Decimal::from(100));
        assert_eq!(operation.mass_g, Decimal::from(100));
        assert_eq!(
            operation.source_amount,
            expected_amounts[nutrient.nutrient.as_str()]
        );
    }
    assert_eq!(repository.snapshots().await.len(), 1);
}

#[tokio::test]
async fn one_bat_rice_stops_for_explicit_grams_clarification() {
    let evidence = ReviewedRiceEvidence::load_committed().expect("reviewed evidence must load");
    let repository = Arc::new(InMemoryAnalysisRepository::default());
    let service = MealAnalysisService::new(
        FixtureParser,
        ReviewedRiceFoodProvider::new(evidence),
        EvaluationPortionProvider,
        SharedInMemoryRepository(Arc::clone(&repository)),
        evaluation_versions(),
        requested_nutrients(),
    );

    let outcome = service
        .execute(AnalysisRequest {
            text: "1 bát cơm trắng".to_owned(),
            locale: "vi-VN".to_owned(),
            idempotency: None,
            owner_id: None,
        })
        .await
        .expect("unsupported household portion should ask for clarification");
    let AnalysisOutcome::NeedsClarification(clarification) = outcome else {
        panic!("unsupported bát must not be assigned a mass");
    };

    assert_eq!(clarification.status, AnalysisStatus::NeedsClarification);
    assert_eq!(clarification.question.dimension, "portion");
    assert_eq!(
        clarification
            .question
            .options
            .iter()
            .map(|option| option.id.as_str())
            .collect::<Vec<_>>(),
        ["grams", "unknown"]
    );
    assert_eq!(
        clarification.versions.clarification_policy_version,
        "clarification-portion-0.2.0"
    );
    let serialized = serde_json::to_value(&clarification)
        .expect("clarification must serialize without a mass estimate");
    assert!(serialized.get("mass_g").is_none());
    assert!(serialized.get("estimated_mass_g").is_none());
    assert!(repository.snapshots().await.is_empty());
}

#[tokio::test]
async fn more_specific_rice_identity_does_not_inherit_generic_reviewed_evidence() {
    let evidence = ReviewedRiceEvidence::load_committed().expect("reviewed evidence must load");
    let provider = ReviewedRiceFoodProvider::new(evidence);
    let more_specific_phrase = ParsedMealItem {
        source_text: "100 g cơm trắng jasmine".to_owned(),
        food_phrase: "cơm trắng jasmine".to_owned(),
        quantity: Some(Decimal::from(100)),
        unit_phrase: Some("g".to_owned()),
        modifiers: Vec::new(),
    };
    let outcome = provider.resolve_food("vi-VN", &more_specific_phrase).await;
    assert!(matches!(
        outcome,
        Err(ApplicationError::InsufficientEvidence(_))
    ));

    let unrepresented_modifier = ParsedMealItem {
        source_text: "100 g cơm trắng jasmine".to_owned(),
        food_phrase: "cơm trắng".to_owned(),
        quantity: Some(Decimal::from(100)),
        unit_phrase: Some("g".to_owned()),
        modifiers: vec!["jasmine".to_owned()],
    };
    let outcome = provider
        .resolve_food("vi-VN", &unrepresented_modifier)
        .await;
    assert!(matches!(
        outcome,
        Err(ApplicationError::InsufficientEvidence(_))
    ));
}
