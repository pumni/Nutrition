use crate::{
    AnalysisItemSnapshot, AnalysisOutcome, AnalysisRepository, AnalysisRequest, AnalysisSnapshot,
    AnalysisStatus, ApplicationError, BehaviorVersions, ClarificationAnalysis,
    ClarificationContext, ClarificationOption, ClarificationQuestion, FoodEvidenceProvider,
    MealTextParser, ParseRequest, PortionEvidenceProvider,
};
use async_trait::async_trait;
use domain::{
    AnalysisId, AnalysisRevisionId, CalculationInput, ClarificationQuestionId,
    DeterministicCalculator, EvidenceQuality, NutrientCode, ResolvedItemInput,
};

#[async_trait]
pub trait AnalyzeMeal: Send + Sync {
    async fn execute(&self, request: AnalysisRequest) -> Result<AnalysisOutcome, ApplicationError>;
}

pub struct MealAnalysisService<P, F, O, R> {
    parser: P,
    food_evidence: F,
    portion_evidence: O,
    repository: R,
    versions: BehaviorVersions,
    requested_nutrients: Vec<NutrientCode>,
}

impl<P, F, O, R> MealAnalysisService<P, F, O, R> {
    #[must_use]
    pub fn new(
        parser: P,
        food_evidence: F,
        portion_evidence: O,
        repository: R,
        versions: BehaviorVersions,
        requested_nutrients: Vec<NutrientCode>,
    ) -> Self {
        Self {
            parser,
            food_evidence,
            portion_evidence,
            repository,
            versions,
            requested_nutrients,
        }
    }
}

#[async_trait]
impl<P, F, O, R> AnalyzeMeal for MealAnalysisService<P, F, O, R>
where
    P: MealTextParser,
    F: FoodEvidenceProvider,
    O: PortionEvidenceProvider,
    R: AnalysisRepository,
{
    async fn execute(&self, request: AnalysisRequest) -> Result<AnalysisOutcome, ApplicationError> {
        validate_request(&request)?;
        let parsed = self
            .parser
            .parse(ParseRequest {
                text: request.text.clone(),
                locale: request.locale.clone(),
            })
            .await?;
        validate_parsed_item_count(parsed.items.len())?;

        let parsed_item_count = parsed.items.len();
        let mut item_snapshots = Vec::with_capacity(parsed_item_count);
        let mut calculation_items = Vec::with_capacity(parsed_item_count);
        for item in parsed.items {
            let food = self
                .food_evidence
                .resolve_food(&request.locale, &item)
                .await?;
            let portion = match self
                .portion_evidence
                .resolve_portion(&request.locale, &item, food.food_id)
                .await
            {
                Ok(portion) => portion,
                Err(ApplicationError::InsufficientEvidence(_)) if parsed_item_count == 1 => {
                    let suggestions = self
                        .portion_evidence
                        .suggestions(&request.locale, food.food_id)
                        .await?;
                    if suggestions.is_empty() {
                        return Err(ApplicationError::InsufficientEvidence(
                            "no portion clarification options are available".to_owned(),
                        ));
                    }
                    let options = suggestions
                        .into_iter()
                        .map(|suggestion| ClarificationOption {
                            id: format!("unit:{}", suggestion.unit),
                            label: suggestion.label,
                        })
                        .collect();
                    let clarification = build_portion_clarification(
                        &request,
                        &self.versions,
                        item,
                        food.food_id,
                        food.food_name,
                        options,
                    );
                    self.repository.save_clarification(&clarification).await?;
                    return Ok(AnalysisOutcome::NeedsClarification(clarification));
                }
                Err(error) => return Err(error),
            };
            let profile_id = food.composition.profile_id;
            let evidence_quality = weaker_quality(food.quality, portion.quality);
            calculation_items.push(ResolvedItemInput {
                food_id: food.food_id,
                mass: portion.mass.clone(),
                composition: food.composition,
                recipe_version_id: None,
            });
            item_snapshots.push(AnalysisItemSnapshot {
                source_text: item.source_text,
                food_id: food.food_id,
                food_name: food.food_name,
                quantity: item.quantity,
                unit_phrase: item.unit_phrase,
                profile_id,
                portion_observation_id: portion.mass.evidence_id,
                estimated_mass_g: portion.mass.central_g,
                lower_mass_g: portion.mass.lower_g,
                upper_mass_g: portion.mass.upper_g,
                mass_resolution_method: portion.mass.method,
                evidence_quality,
                assumptions: portion.assumptions,
            });
        }

        let calculation = DeterministicCalculator::calculate(&CalculationInput {
            engine_version: self.versions.calculation_engine_version.clone(),
            requested_nutrients: self.requested_nutrients.clone(),
            items: calculation_items,
        })
        .map_err(|error| ApplicationError::Calculation(error.to_string()))?;

        let snapshot = AnalysisSnapshot {
            analysis_id: AnalysisId::new(),
            revision_id: AnalysisRevisionId::new(),
            revision_number: 1,
            status: AnalysisStatus::Completed,
            locale: request.locale,
            versions: self.versions.clone(),
            items: item_snapshots,
            requested_nutrients: self.requested_nutrients.clone(),
            calculation,
            is_estimate: true,
            idempotency: request.idempotency,
        };
        self.repository.save(&snapshot).await?;
        Ok(AnalysisOutcome::Completed(snapshot))
    }
}

fn build_portion_clarification(
    request: &AnalysisRequest,
    versions: &BehaviorVersions,
    item: crate::ParsedMealItem,
    food_id: domain::FoodId,
    food_name: String,
    mut options: Vec<ClarificationOption>,
) -> ClarificationAnalysis {
    options.push(ClarificationOption {
        id: "grams".to_owned(),
        label: "Nhập khối lượng gam".to_owned(),
    });
    options.push(ClarificationOption {
        id: "unknown".to_owned(),
        label: "Không chắc".to_owned(),
    });
    ClarificationAnalysis {
        analysis_id: AnalysisId::new(),
        revision_id: AnalysisRevisionId::new(),
        revision_number: 1,
        status: AnalysisStatus::NeedsClarification,
        locale: request.locale.clone(),
        versions: versions.clone(),
        question: ClarificationQuestion {
            id: ClarificationQuestionId::new(),
            dimension: "portion".to_owned(),
            prompt: format!("Bạn có thể làm rõ khẩu phần của “{food_name}” không?"),
            options,
        },
        context: ClarificationContext {
            item,
            food_id,
            food_name,
        },
        idempotency: request.idempotency.clone(),
    }
}

const fn weaker_quality(left: EvidenceQuality, right: EvidenceQuality) -> EvidenceQuality {
    if quality_rank(left) >= quality_rank(right) {
        left
    } else {
        right
    }
}

const fn quality_rank(value: EvidenceQuality) -> u8 {
    match value {
        EvidenceQuality::A => 0,
        EvidenceQuality::B => 1,
        EvidenceQuality::C => 2,
        EvidenceQuality::D => 3,
        EvidenceQuality::U => 4,
    }
}

fn validate_request(request: &AnalysisRequest) -> Result<(), ApplicationError> {
    let length = request.text.chars().count();
    if length == 0 || length > 2_000 {
        return Err(ApplicationError::InvalidInput(
            "meal text must contain between 1 and 2000 characters".to_owned(),
        ));
    }
    if request.locale.trim().is_empty() || request.locale.len() > 32 {
        return Err(ApplicationError::InvalidInput(
            "locale is required and must be at most 32 bytes".to_owned(),
        ));
    }
    Ok(())
}

fn validate_parsed_item_count(count: usize) -> Result<(), ApplicationError> {
    if count == 0 || count > 10 {
        return Err(ApplicationError::InvalidInput(
            "parser must return between 1 and 10 consumed items".to_owned(),
        ));
    }
    Ok(())
}
