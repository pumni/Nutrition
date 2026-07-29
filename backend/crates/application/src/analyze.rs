use crate::{
    AnalysisItemSnapshot, AnalysisRepository, AnalysisRequest, AnalysisSnapshot, AnalysisStatus,
    ApplicationError, BehaviorVersions, CatalogEvidenceProvider, MealTextParser, ParseRequest,
};
use async_trait::async_trait;
use domain::{
    AnalysisId, AnalysisRevisionId, CalculationInput, DeterministicCalculator, NutrientCode,
    ResolvedItemInput,
};

#[async_trait]
pub trait AnalyzeMeal: Send + Sync {
    async fn execute(&self, request: AnalysisRequest)
    -> Result<AnalysisSnapshot, ApplicationError>;
}

pub struct DirectAnalysisService<P, E, R> {
    parser: P,
    evidence: E,
    repository: R,
    versions: BehaviorVersions,
    requested_nutrients: Vec<NutrientCode>,
}

impl<P, E, R> DirectAnalysisService<P, E, R> {
    #[must_use]
    pub fn new(
        parser: P,
        evidence: E,
        repository: R,
        versions: BehaviorVersions,
        requested_nutrients: Vec<NutrientCode>,
    ) -> Self {
        Self {
            parser,
            evidence,
            repository,
            versions,
            requested_nutrients,
        }
    }
}

#[async_trait]
impl<P, E, R> AnalyzeMeal for DirectAnalysisService<P, E, R>
where
    P: MealTextParser,
    E: CatalogEvidenceProvider,
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
            let evidence = self.evidence.resolve_direct(&request.locale, &item).await?;
            calculation_items.push(ResolvedItemInput {
                food_id: evidence.food_id,
                mass: evidence.mass.clone(),
                composition: evidence.composition,
                recipe_version_id: None,
            });
            item_snapshots.push(AnalysisItemSnapshot {
                source_text: item.source_text,
                food_id: evidence.food_id,
                food_name: evidence.food_name,
                estimated_mass_g: evidence.mass.central_g,
                evidence_quality: evidence.quality,
                assumptions: evidence.assumptions,
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
