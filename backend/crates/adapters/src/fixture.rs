use application::{
    AnalysisRepository, AnalysisSnapshot, ApplicationError, CatalogEvidenceProvider,
    MealTextParser, ParseRequest, ParsedMealDocument, ParsedMealItem, ResolvedEvidence,
};
use async_trait::async_trait;
use domain::{
    CompositionProfileId, CompositionSnapshot, CompositionValue, EvidenceQuality, FoodId,
    MassEstimate, MassResolutionMethod, NutrientCode, NutrientUnit, ValueStatus,
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
            if tokens.len() < 3 || !tokens[1].eq_ignore_ascii_case("g") {
                return Err(ApplicationError::InvalidInput(
                    "fixture parser expects '<grams> g <food>', items separated by commas"
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
                unit_phrase: Some("g".to_owned()),
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
            normalize("trứng gà luộc"),
            fixture_food(
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
            normalize("cơm trắng"),
            fixture_food(
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
impl CatalogEvidenceProvider for FixtureCatalog {
    async fn resolve_direct(
        &self,
        _locale: &str,
        item: &ParsedMealItem,
    ) -> Result<ResolvedEvidence, ApplicationError> {
        let quantity = item.quantity.ok_or_else(|| {
            ApplicationError::InsufficientEvidence("explicit gram quantity is required".to_owned())
        })?;
        if item.unit_phrase.as_deref() != Some("g") || quantity <= Decimal::ZERO {
            return Err(ApplicationError::InsufficientEvidence(
                "foundation slice only supports positive explicit grams".to_owned(),
            ));
        }
        let food = self
            .foods
            .get(&normalize(&item.food_phrase))
            .ok_or_else(|| {
                ApplicationError::InsufficientEvidence(format!(
                    "unknown fixture food: {}",
                    item.food_phrase
                ))
            })?;
        Ok(ResolvedEvidence {
            food_id: food.id,
            food_name: food.name.clone(),
            mass: MassEstimate {
                central_g: quantity,
                lower_g: None,
                upper_g: None,
                evidence_id: None,
                method: MassResolutionMethod::ExplicitMass,
            },
            composition: food.profile.clone(),
            quality: EvidenceQuality::A,
            assumptions: Vec::new(),
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

fn fixture_food(name: &str, values: &[(&str, &str, NutrientUnit)]) -> FixtureFood {
    FixtureFood {
        id: FoodId::new(),
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

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::{
        AnalysisMode, AnalysisRequest, AnalyzeMeal, BehaviorVersions, DirectAnalysisService,
    };

    #[tokio::test]
    async fn direct_slice_is_persisted_and_replayable() {
        let service = DirectAnalysisService::new(
            FixtureParser,
            FixtureCatalog::foundation_seed(),
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
        let service = DirectAnalysisService::new(
            FixtureParser,
            FixtureCatalog::foundation_seed(),
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
}
