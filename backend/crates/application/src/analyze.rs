use crate::{
    AnalysisItemSnapshot, AnalysisRepository, AnalysisRequest, AnalysisSnapshot, AnalysisStatus,
    ApplicationError, BehaviorVersions, FoodEvidenceProvider, MealTextParser, ParseRequest,
    PortionEvidenceProvider,
};
use async_trait::async_trait;
use domain::{
    AnalysisId, AnalysisRevisionId, CalculationInput, DeterministicCalculator, EvidenceQuality,
    NutrientCode, ResolvedItemInput,
};

#[async_trait]
pub trait AnalyzeMeal: Send + Sync {
    async fn execute(&self, request: AnalysisRequest)
    -> Result<AnalysisSnapshot, ApplicationError>;
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
    async fn execute(
        &self,
        request: AnalysisRequest,
    ) -> Result<AnalysisSnapshot, ApplicationError> {
        validate_request(&request)?;
        let parsed = self
            .parser
            .parse(ParseRequest {
                text: request.text,
                locale: request.locale.clone(),
            })
            .await?;
        if parsed.items.is_empty() || parsed.items.len() > 10 {
            return Err(ApplicationError::InvalidInput(
                "parser must return between 1 and 10 consumed items".to_owned(),
            ));
        }

        let mut item_snapshots = Vec::with_capacity(parsed.items.len());
        let mut calculation_items = Vec::with_capacity(parsed.items.len());
        for item in parsed.items {
            let food = self
                .food_evidence
                .resolve_food(&request.locale, &item)
                .await?;
            let portion = self
                .portion_evidence
                .resolve_portion(&request.locale, &item, food.food_id)
                .await?;
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
        };
        self.repository.save(&snapshot).await?;
        Ok(snapshot)
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
