use application::{
    AnalysisRepository, AnalysisSnapshot, ApplicationError, FoodEvidenceProvider, MealTextParser,
    ParseRequest, ParsedMealDocument, ParsedMealItem, PortionEvidenceProvider,
    ResolvedFoodEvidence, ResolvedPortionEvidence, normalize_vi_search_key,
};
use async_trait::async_trait;
use domain::{
    CompositionProfileId, CompositionSnapshot, CompositionValue, EvidenceQuality, FoodId,
    MassEstimate, MassResolutionMethod, NutrientCode, NutrientUnit, PortionObservationId,
    ValueStatus,
};
use rust_decimal::Decimal;
use std::{collections::BTreeMap, str::FromStr};
use tokio::sync::RwLock;

#[derive(Clone, Copy, Debug, Default)]
pub struct FixtureParser;

#[async_trait]
impl MealTextParser for FixtureParser {
    async fn parse(&self, request: ParseRequest) -> Result<ParsedMealDocument, ApplicationError> {
        let mut items = Vec::new();
        for raw_item in request
            .text
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let tokens = raw_item.split_whitespace().collect::<Vec<_>>();
            if tokens.len() < 3 {
                return Err(ApplicationError::InvalidInput(
                    "fixture parser expects '<quantity> <unit> <food>', items separated by commas"
                        .to_owned(),
                ));
            }
            let quantity = Decimal::from_str(tokens[0]).map_err(|_| {
                ApplicationError::InvalidInput(
                    "fixture quantity must be a decimal number".to_owned(),
                )
            })?;
            items.push(ParsedMealItem {
                source_text: raw_item.to_owned(),
                food_phrase: tokens[2..].join(" "),
                quantity: Some(quantity),
                unit_phrase: Some(normalize_vi_search_key(tokens[1])),
                modifiers: Vec::new(),
            });
        }
        Ok(ParsedMealDocument {
            language: request.locale,
            items,
            warnings: vec!["fixture_parser_active".to_owned()],
        })
    }
}

#[derive(Clone)]
struct FixtureFood {
    id: FoodId,
    name: String,
    profile: CompositionSnapshot,
}

pub struct FixtureCatalog {
    foods: BTreeMap<String, FixtureFood>,
}

impl FixtureCatalog {
    #[must_use]
    pub fn foundation_seed() -> Self {
        let mut foods = BTreeMap::new();
        foods.insert(
            normalize_vi_search_key("trứng gà luộc"),
            fixture_food(
                FoodId::from_u128(0x0198_f100_0000_7000_8000_0000_0000_0020),
                "Trứng gà luộc",
                &[
                    ("energy_kcal", "155", NutrientUnit::Kilocalorie),
                    ("protein_g", "12.6", NutrientUnit::Gram),
                    ("carbohydrate_g", "1.12", NutrientUnit::Gram),
                    ("fat_g", "10.6", NutrientUnit::Gram),
                ],
            ),
        );
        foods.insert(
            normalize_vi_search_key("cơm trắng"),
            fixture_food(
                FoodId::from_u128(0x0198_f100_0000_7000_8000_0000_0000_0021),
                "Cơm trắng",
                &[
                    ("energy_kcal", "130", NutrientUnit::Kilocalorie),
                    ("protein_g", "2.69", NutrientUnit::Gram),
                    ("carbohydrate_g", "28.2", NutrientUnit::Gram),
                    ("fat_g", "0.28", NutrientUnit::Gram),
                ],
            ),
        );
        Self { foods }
    }
}

#[async_trait]
impl FoodEvidenceProvider for FixtureCatalog {
    async fn resolve_food(
        &self,
        _locale: &str,
        item: &ParsedMealItem,
    ) -> Result<ResolvedFoodEvidence, ApplicationError> {
        let food = self
            .foods
            .get(&normalize_vi_search_key(&item.food_phrase))
            .ok_or_else(|| {
                ApplicationError::InsufficientEvidence(format!(
                    "unknown fixture food: {}",
                    item.food_phrase
                ))
            })?;
        Ok(ResolvedFoodEvidence {
            food_id: food.id,
            food_name: food.name.clone(),
            composition: food.profile.clone(),
            quality: EvidenceQuality::A,
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FixturePortionEvidenceProvider;

#[async_trait]
impl PortionEvidenceProvider for FixturePortionEvidenceProvider {
    async fn resolve_portion(
        &self,
        _locale: &str,
        item: &ParsedMealItem,
        food_id: FoodId,
    ) -> Result<ResolvedPortionEvidence, ApplicationError> {
        let quantity = item.quantity.ok_or_else(|| {
            ApplicationError::InsufficientEvidence("explicit quantity is required".to_owned())
        })?;
        if quantity <= Decimal::ZERO {
            return Err(ApplicationError::InsufficientEvidence(
                "quantity must be positive".to_owned(),
            ));
        }
        let unit = item.unit_phrase.as_deref().ok_or_else(|| {
            ApplicationError::InsufficientEvidence("portion unit is required".to_owned())
        })?;
        if unit == "g" {
            return Ok(ResolvedPortionEvidence {
                mass: MassEstimate {
                    central_g: quantity,
                    lower_g: None,
                    upper_g: None,
                    evidence_id: None,
                    method: MassResolutionMethod::ExplicitMass,
                },
                quality: EvidenceQuality::A,
                assumptions: Vec::new(),
            });
        }

        let egg_id = FoodId::from_u128(0x0198_f100_0000_7000_8000_0000_0000_0020);
        let rice_id = FoodId::from_u128(0x0198_f100_0000_7000_8000_0000_0000_0021);
        let observation = match (food_id, unit) {
            (id, "quả") if id == egg_id => (
                Decimal::from(50),
                Decimal::from(45),
                Decimal::from(60),
                PortionObservationId::from_u128(0x0198_f100_0000_7000_8000_0000_0000_0050),
            ),
            (id, "bát") if id == rice_id => (
                Decimal::from(150),
                Decimal::from(120),
                Decimal::from(200),
                PortionObservationId::from_u128(0x0198_f100_0000_7000_8000_0000_0000_0051),
            ),
            _ => {
                return Err(ApplicationError::InsufficientEvidence(format!(
                    "no contextual portion evidence for unit: {unit}"
                )));
            }
        };
        Ok(ResolvedPortionEvidence {
            mass: MassEstimate {
                central_g: quantity * observation.0,
                lower_g: Some(quantity * observation.1),
                upper_g: Some(quantity * observation.2),
                evidence_id: Some(observation.3),
                method: MassResolutionMethod::PortionObservation,
            },
            quality: EvidenceQuality::C,
            assumptions: vec!["Sử dụng quan sát khẩu phần thử nghiệm".to_owned()],
        })
    }
}

#[derive(Default)]
pub struct InMemoryAnalysisRepository {
    snapshots: RwLock<Vec<AnalysisSnapshot>>,
}

impl InMemoryAnalysisRepository {
    pub async fn snapshots(&self) -> Vec<AnalysisSnapshot> {
        self.snapshots.read().await.clone()
    }
}

#[async_trait]
impl AnalysisRepository for InMemoryAnalysisRepository {
    async fn save(&self, snapshot: &AnalysisSnapshot) -> Result<(), ApplicationError> {
        self.snapshots.write().await.push(snapshot.clone());
        Ok(())
    }
}

fn fixture_food(id: FoodId, name: &str, values: &[(&str, &str, NutrientUnit)]) -> FixtureFood {
    FixtureFood {
        id,
        name: name.to_owned(),
        profile: CompositionSnapshot {
            profile_id: CompositionProfileId::new(),
            basis_g: Decimal::ONE_HUNDRED,
            quality: EvidenceQuality::A,
            values: values
                .iter()
                .map(|(code, amount, unit)| CompositionValue {
                    nutrient: NutrientCode::new(*code).expect("fixture nutrient code is valid"),
                    amount: Some(
                        Decimal::from_str(amount).expect("fixture nutrient amount is valid"),
                    ),
                    lower_amount: None,
                    upper_amount: None,
                    unit: *unit,
                    status: ValueStatus::Measured,
                })
                .collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::{
        AnalysisMode, AnalysisRequest, AnalyzeMeal, BehaviorVersions, MealAnalysisService,
    };

    #[tokio::test]
    async fn direct_slice_is_persisted_and_replayable() {
        let service = MealAnalysisService::new(
            FixtureParser,
            FixtureCatalog::foundation_seed(),
            FixturePortionEvidenceProvider,
            InMemoryAnalysisRepository::default(),
            BehaviorVersions::default(),
            vec![
                NutrientCode::new("energy_kcal").expect("valid code"),
                NutrientCode::new("protein_g").expect("valid code"),
                NutrientCode::new("carbohydrate_g").expect("valid code"),
                NutrientCode::new("fat_g").expect("valid code"),
            ],
        );
        let first = service
            .execute(AnalysisRequest {
                text: "100 g trứng gà luộc, 150 g cơm trắng".to_owned(),
                locale: "vi-VN".to_owned(),
                mode: AnalysisMode::Balanced,
            })
            .await
            .expect("direct slice should complete");

        assert!(first.is_estimate);
        assert_eq!(first.items.len(), 2);
        assert_eq!(first.calculation.totals.len(), 4);
        let protein = first
            .calculation
            .totals
            .iter()
            .find(|total| total.nutrient.as_str() == "protein_g")
            .expect("protein total must exist");
        assert_eq!(
            protein.amount,
            Some(Decimal::from_str("16.635").expect("valid expected decimal"))
        );
    }

    #[tokio::test]
    async fn unknown_food_is_not_force_matched() {
        let service = MealAnalysisService::new(
            FixtureParser,
            FixtureCatalog::foundation_seed(),
            FixturePortionEvidenceProvider,
            InMemoryAnalysisRepository::default(),
            BehaviorVersions::default(),
            vec![NutrientCode::new("energy_kcal").expect("valid code")],
        );
        let error = service
            .execute(AnalysisRequest {
                text: "100 g món không tồn tại".to_owned(),
                locale: "vi-VN".to_owned(),
                mode: AnalysisMode::Balanced,
            })
            .await
            .expect_err("unknown food must not resolve");

        assert!(matches!(error, ApplicationError::InsufficientEvidence(_)));
    }

    #[tokio::test]
    async fn contextual_portions_produce_mass_bounds() {
        let service = MealAnalysisService::new(
            FixtureParser,
            FixtureCatalog::foundation_seed(),
            FixturePortionEvidenceProvider,
            InMemoryAnalysisRepository::default(),
            BehaviorVersions::default(),
            vec![NutrientCode::new("energy_kcal").expect("valid code")],
        );
        let result = service
            .execute(AnalysisRequest {
                text: "2 quả trứng gà luộc, 1 bát cơm trắng".to_owned(),
                locale: "vi-VN".to_owned(),
                mode: AnalysisMode::Balanced,
            })
            .await
            .expect("contextual portions should resolve");

        assert_eq!(result.items[0].estimated_mass_g, Decimal::from(100));
        assert_eq!(result.items[0].lower_mass_g, Some(Decimal::from(90)));
        assert_eq!(result.items[0].upper_mass_g, Some(Decimal::from(120)));
        assert_eq!(result.items[1].estimated_mass_g, Decimal::from(150));
        assert_eq!(result.items[1].lower_mass_g, Some(Decimal::from(120)));
        assert_eq!(result.items[1].upper_mass_g, Some(Decimal::from(200)));
        assert!(result.calculation.totals[0].lower_amount.is_some());
        assert!(result.calculation.totals[0].upper_amount.is_some());
    }
}
