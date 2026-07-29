use crate::{
    AnalysisItemSnapshot, AnalysisRepository, AnalysisSnapshot, AnalysisSnapshotReader,
    AnalysisStatus, ApplicationError, BehaviorVersions, ClarificationAnswerRequest,
    CorrectionRequest, FoodEvidenceProvider, ParsedMealItem, PortionEvidenceProvider,
};
use async_trait::async_trait;
use domain::{
    AnalysisId, AnalysisRevisionId, CalculationInput, DeterministicCalculator, EvidenceQuality,
    NutrientCode, ResolvedItemInput,
};
use rust_decimal::Decimal;
use std::collections::BTreeMap;

#[async_trait]
pub trait AnswerClarification: Send + Sync {
    async fn answer(
        &self,
        analysis_id: AnalysisId,
        request: ClarificationAnswerRequest,
    ) -> Result<AnalysisSnapshot, ApplicationError>;
}

#[async_trait]
pub trait CorrectAnalysis: Send + Sync {
    async fn correct(
        &self,
        analysis_id: AnalysisId,
        request: CorrectionRequest,
    ) -> Result<AnalysisSnapshot, ApplicationError>;
}

pub struct AnalysisRevisionService<F, O, R> {
    food_evidence: F,
    portion_evidence: O,
    repository: R,
    versions: BehaviorVersions,
    requested_nutrients: Vec<NutrientCode>,
}

impl<F, O, R> AnalysisRevisionService<F, O, R> {
    #[must_use]
    pub fn new(
        food_evidence: F,
        portion_evidence: O,
        repository: R,
        versions: BehaviorVersions,
        requested_nutrients: Vec<NutrientCode>,
    ) -> Self {
        Self {
            food_evidence,
            portion_evidence,
            repository,
            versions,
            requested_nutrients,
        }
    }
}

#[async_trait]
impl<F, O, R> AnswerClarification for AnalysisRevisionService<F, O, R>
where
    F: FoodEvidenceProvider,
    O: PortionEvidenceProvider,
    R: AnalysisRepository + AnalysisSnapshotReader,
{
    async fn answer(
        &self,
        analysis_id: AnalysisId,
        request: ClarificationAnswerRequest,
    ) -> Result<AnalysisSnapshot, ApplicationError> {
        let pending = self
            .repository
            .find_open_clarification(analysis_id)
            .await?
            .ok_or(ApplicationError::StaleClarification)?;
        if pending.revision_id != request.expected_revision_id
            || pending.question.id != request.question_id
            || !pending
                .question
                .options
                .iter()
                .any(|option| option.id == request.option_id)
        {
            return Err(ApplicationError::StaleClarification);
        }
        let item = clarification_item(&pending, &request)?;
        let snapshot = self
            .build_snapshot(
                analysis_id,
                pending.revision_number + 1,
                pending.locale,
                vec![item],
            )
            .await?;
        self.repository
            .append_clarification_answer(&request, &snapshot)
            .await?;
        Ok(snapshot)
    }
}

#[async_trait]
impl<F, O, R> CorrectAnalysis for AnalysisRevisionService<F, O, R>
where
    F: FoodEvidenceProvider,
    O: PortionEvidenceProvider,
    R: AnalysisRepository + AnalysisSnapshotReader,
{
    async fn correct(
        &self,
        analysis_id: AnalysisId,
        request: CorrectionRequest,
    ) -> Result<AnalysisSnapshot, ApplicationError> {
        validate_correction(&request)?;
        let current = self
            .repository
            .find(analysis_id)
            .await?
            .ok_or(ApplicationError::NotFound)?;
        if current.revision_id != request.base_revision_id {
            return Err(ApplicationError::RevisionConflict);
        }
        let corrections = request
            .item_corrections
            .iter()
            .map(|correction| (correction.item_index, correction))
            .collect::<BTreeMap<_, _>>();
        if corrections
            .keys()
            .any(|index| *index >= current.items.len())
        {
            return Err(ApplicationError::InvalidInput(
                "correction item index is out of range".to_owned(),
            ));
        }
        let items = current
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let (quantity, unit) = corrections.get(&index).map_or_else(
                    || {
                        (
                            item.quantity.unwrap_or(item.estimated_mass_g),
                            item.unit_phrase.clone().unwrap_or_else(|| "g".to_owned()),
                        )
                    },
                    |correction| (correction.quantity, correction.unit.clone()),
                );
                ParsedMealItem {
                    source_text: format!("{quantity} {unit} {}", item.food_name),
                    food_phrase: item.food_name.clone(),
                    quantity: Some(quantity),
                    unit_phrase: Some(unit),
                    modifiers: Vec::new(),
                }
            })
            .collect();
        let snapshot = self
            .build_snapshot(
                analysis_id,
                current.revision_number + 1,
                current.locale,
                items,
            )
            .await?;
        self.repository
            .append_correction(&request, &snapshot)
            .await?;
        Ok(snapshot)
    }
}

impl<F, O, R> AnalysisRevisionService<F, O, R>
where
    F: FoodEvidenceProvider,
    O: PortionEvidenceProvider,
{
    async fn build_snapshot(
        &self,
        analysis_id: AnalysisId,
        revision_number: u32,
        locale: String,
        items: Vec<ParsedMealItem>,
    ) -> Result<AnalysisSnapshot, ApplicationError> {
        let mut item_snapshots = Vec::with_capacity(items.len());
        let mut calculation_items = Vec::with_capacity(items.len());
        for item in items {
            let food = self.food_evidence.resolve_food(&locale, &item).await?;
            let portion = self
                .portion_evidence
                .resolve_portion(&locale, &item, food.food_id)
                .await?;
            let quality = weaker_quality(food.quality, portion.quality);
            let profile_id = food.composition.profile_id;
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
                evidence_quality: quality,
                assumptions: portion.assumptions,
            });
        }
        let calculation = DeterministicCalculator::calculate(&CalculationInput {
            engine_version: self.versions.calculation_engine_version.clone(),
            requested_nutrients: self.requested_nutrients.clone(),
            items: calculation_items,
        })
        .map_err(|error| ApplicationError::Calculation(error.to_string()))?;
        Ok(AnalysisSnapshot {
            analysis_id,
            revision_id: AnalysisRevisionId::new(),
            revision_number,
            status: AnalysisStatus::Completed,
            locale,
            versions: self.versions.clone(),
            items: item_snapshots,
            requested_nutrients: self.requested_nutrients.clone(),
            calculation,
            is_estimate: true,
            idempotency: None,
            owner_id: None,
        })
    }
}

fn clarification_item(
    pending: &crate::ClarificationAnalysis,
    request: &ClarificationAnswerRequest,
) -> Result<ParsedMealItem, ApplicationError> {
    let (quantity, unit) = if request.option_id == "grams" {
        let mass = request.mass_g.ok_or_else(|| {
            ApplicationError::InvalidInput("mass_g is required for grams option".to_owned())
        })?;
        (mass, "g".to_owned())
    } else if request.option_id == "unknown" {
        return Err(ApplicationError::InsufficientEvidence(
            "user could not clarify the portion".to_owned(),
        ));
    } else {
        let unit = request
            .option_id
            .strip_prefix("unit:")
            .ok_or_else(|| ApplicationError::InvalidInput("invalid option ID".to_owned()))?;
        let quantity = pending.context.item.quantity.ok_or_else(|| {
            ApplicationError::InvalidInput("original quantity is missing".to_owned())
        })?;
        (quantity, unit.to_owned())
    };
    if quantity <= Decimal::ZERO {
        return Err(ApplicationError::InvalidInput(
            "clarified quantity must be positive".to_owned(),
        ));
    }
    Ok(ParsedMealItem {
        source_text: format!("{quantity} {unit} {}", pending.context.food_name),
        food_phrase: pending.context.food_name.clone(),
        quantity: Some(quantity),
        unit_phrase: Some(unit),
        modifiers: pending.context.item.modifiers.clone(),
    })
}

fn validate_correction(request: &CorrectionRequest) -> Result<(), ApplicationError> {
    if request.item_corrections.is_empty() || request.item_corrections.len() > 10 {
        return Err(ApplicationError::InvalidInput(
            "correction must contain between 1 and 10 items".to_owned(),
        ));
    }
    let mut indexes = request
        .item_corrections
        .iter()
        .map(|correction| correction.item_index)
        .collect::<Vec<_>>();
    indexes.sort_unstable();
    indexes.dedup();
    if indexes.len() != request.item_corrections.len()
        || request
            .item_corrections
            .iter()
            .any(|correction| correction.quantity <= Decimal::ZERO || correction.unit.is_empty())
    {
        return Err(ApplicationError::InvalidInput(
            "correction items must be unique and contain positive quantities and units".to_owned(),
        ));
    }
    Ok(())
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
