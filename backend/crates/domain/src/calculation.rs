use crate::{CompositionProfileId, NutrientCode, NutrientUnit, ResolvedItemInput, ValueStatus};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CalculationInput {
    pub engine_version: String,
    pub requested_nutrients: Vec<NutrientCode>,
    pub items: Vec<ResolvedItemInput>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CalculationOperation {
    pub profile_id: CompositionProfileId,
    pub source_amount: Decimal,
    pub basis_g: Decimal,
    pub mass_g: Decimal,
    pub result: Decimal,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NutrientCalculationResult {
    pub nutrient: NutrientCode,
    pub unit: NutrientUnit,
    pub amount: Option<Decimal>,
    pub lower_amount: Option<Decimal>,
    pub upper_amount: Option<Decimal>,
    pub source_status: ValueStatus,
    pub operation: Option<CalculationOperation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ItemCalculationResult {
    pub food_id: crate::FoodId,
    pub mass_g: Decimal,
    pub nutrients: Vec<NutrientCalculationResult>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TotalNutrientResult {
    pub nutrient: NutrientCode,
    pub unit: NutrientUnit,
    pub amount: Option<Decimal>,
    pub lower_amount: Option<Decimal>,
    pub upper_amount: Option<Decimal>,
    pub completeness_ratio: Decimal,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CalculationResult {
    pub engine_version: String,
    pub items: Vec<ItemCalculationResult>,
    pub totals: Vec<TotalNutrientResult>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicCalculator;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CalculationError {
    #[error("calculation engine version is required")]
    MissingEngineVersion,
    #[error("at least one item is required")]
    NoItems,
    #[error("mass must be positive and bounds must contain the central value")]
    InvalidMass,
    #[error("composition basis must be positive")]
    InvalidBasis,
    #[error("composition profile contains a duplicate nutrient: {0}")]
    DuplicateNutrient(String),
    #[error("composition amount or bounds are invalid for nutrient: {0}")]
    InvalidCompositionValue(String),
    #[error("composition profile uses inconsistent units for nutrient: {0}")]
    InconsistentNutrientUnit(String),
}

#[derive(Clone)]
struct TotalAccumulator {
    unit: NutrientUnit,
    amount: Decimal,
    lower: Decimal,
    upper: Decimal,
    known_mass: Decimal,
    has_value: bool,
    has_bounds: bool,
}

impl DeterministicCalculator {
    /// Calculates item-level nutrients and completeness-aware totals without external I/O.
    ///
    /// # Errors
    ///
    /// Returns [`CalculationError`] when masses, composition bases, value bounds, nutrient
    /// uniqueness, or canonical units violate the calculation contract.
    pub fn calculate(input: &CalculationInput) -> Result<CalculationResult, CalculationError> {
        if input.engine_version.trim().is_empty() {
            return Err(CalculationError::MissingEngineVersion);
        }
        if input.items.is_empty() {
            return Err(CalculationError::NoItems);
        }

        let requested = input
            .requested_nutrients
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let total_mass = input.items.iter().try_fold(Decimal::ZERO, |sum, item| {
            validate_mass(item)?;
            Ok::<_, CalculationError>(sum + item.mass.central_g)
        })?;

        let mut totals: BTreeMap<NutrientCode, TotalAccumulator> = BTreeMap::new();
        let mut item_results = Vec::with_capacity(input.items.len());
        let mut warnings = Vec::new();

        for item in &input.items {
            item_results.push(calculate_item(
                item,
                &requested,
                &mut totals,
                &mut warnings,
            )?);
        }

        let total_results = requested
            .into_iter()
            .map(|nutrient| {
                let accumulator = totals.get(&nutrient);
                let unit = accumulator.map_or(NutrientUnit::Gram, |value| value.unit);
                TotalNutrientResult {
                    nutrient,
                    unit,
                    amount: accumulator
                        .filter(|value| value.has_value)
                        .map(|value| value.amount),
                    lower_amount: accumulator
                        .filter(|value| value.has_value && value.has_bounds)
                        .map(|value| value.lower),
                    upper_amount: accumulator
                        .filter(|value| value.has_value && value.has_bounds)
                        .map(|value| value.upper),
                    completeness_ratio: accumulator
                        .map_or(Decimal::ZERO, |value| value.known_mass / total_mass),
                }
            })
            .collect();

        warnings.sort();
        warnings.dedup();
        Ok(CalculationResult {
            engine_version: input.engine_version.clone(),
            items: item_results,
            totals: total_results,
            warnings,
        })
    }
}

fn calculate_item(
    item: &ResolvedItemInput,
    requested: &BTreeSet<NutrientCode>,
    totals: &mut BTreeMap<NutrientCode, TotalAccumulator>,
    warnings: &mut Vec<String>,
) -> Result<ItemCalculationResult, CalculationError> {
    if item.composition.basis_g <= Decimal::ZERO {
        return Err(CalculationError::InvalidBasis);
    }

    let mut values = BTreeMap::new();
    for value in &item.composition.values {
        if values.insert(value.nutrient.clone(), value).is_some() {
            return Err(CalculationError::DuplicateNutrient(
                value.nutrient.to_string(),
            ));
        }
        validate_composition_value(value)?;
    }

    let mut nutrient_results = Vec::with_capacity(requested.len());
    for nutrient in requested {
        let Some(value) = values.get(nutrient) else {
            warnings.push(format!("missing_nutrient:{nutrient}"));
            nutrient_results.push(missing_result(
                nutrient,
                NutrientUnit::Gram,
                ValueStatus::Missing,
            ));
            continue;
        };
        let Some(source_amount) = value.amount else {
            warnings.push(format!("missing_nutrient:{nutrient}"));
            nutrient_results.push(missing_result(nutrient, value.unit, value.status));
            continue;
        };

        let amount = source_amount * item.mass.central_g / item.composition.basis_g;
        let has_bounds = value.lower_amount.is_some()
            || value.upper_amount.is_some()
            || item.mass.lower_g.is_some()
            || item.mass.upper_g.is_some();
        let lower = value.lower_amount.unwrap_or(source_amount)
            * item.mass.lower_g.unwrap_or(item.mass.central_g)
            / item.composition.basis_g;
        let upper = value.upper_amount.unwrap_or(source_amount)
            * item.mass.upper_g.unwrap_or(item.mass.central_g)
            / item.composition.basis_g;

        accumulate_total(
            totals,
            nutrient,
            value.unit,
            amount,
            lower,
            upper,
            item.mass.central_g,
            has_bounds,
        )?;
        nutrient_results.push(NutrientCalculationResult {
            nutrient: nutrient.clone(),
            unit: value.unit,
            amount: Some(amount),
            lower_amount: has_bounds.then_some(lower),
            upper_amount: has_bounds.then_some(upper),
            source_status: value.status,
            operation: Some(CalculationOperation {
                profile_id: item.composition.profile_id,
                source_amount,
                basis_g: item.composition.basis_g,
                mass_g: item.mass.central_g,
                result: amount,
            }),
        });
    }

    Ok(ItemCalculationResult {
        food_id: item.food_id,
        mass_g: item.mass.central_g,
        nutrients: nutrient_results,
    })
}

fn missing_result(
    nutrient: &NutrientCode,
    unit: NutrientUnit,
    status: ValueStatus,
) -> NutrientCalculationResult {
    NutrientCalculationResult {
        nutrient: nutrient.clone(),
        unit,
        amount: None,
        lower_amount: None,
        upper_amount: None,
        source_status: status,
        operation: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn accumulate_total(
    totals: &mut BTreeMap<NutrientCode, TotalAccumulator>,
    nutrient: &NutrientCode,
    unit: NutrientUnit,
    amount: Decimal,
    lower: Decimal,
    upper: Decimal,
    mass: Decimal,
    has_bounds: bool,
) -> Result<(), CalculationError> {
    let accumulator = totals.entry(nutrient.clone()).or_insert(TotalAccumulator {
        unit,
        amount: Decimal::ZERO,
        lower: Decimal::ZERO,
        upper: Decimal::ZERO,
        known_mass: Decimal::ZERO,
        has_value: false,
        has_bounds: false,
    });
    if accumulator.unit != unit {
        return Err(CalculationError::InconsistentNutrientUnit(
            nutrient.to_string(),
        ));
    }
    accumulator.amount += amount;
    accumulator.lower += lower;
    accumulator.upper += upper;
    accumulator.known_mass += mass;
    accumulator.has_value = true;
    accumulator.has_bounds |= has_bounds;
    Ok(())
}

fn validate_mass(item: &ResolvedItemInput) -> Result<(), CalculationError> {
    let mass = &item.mass;
    if mass.central_g <= Decimal::ZERO
        || mass
            .lower_g
            .is_some_and(|lower| lower <= Decimal::ZERO || lower > mass.central_g)
        || mass.upper_g.is_some_and(|upper| upper < mass.central_g)
    {
        return Err(CalculationError::InvalidMass);
    }
    Ok(())
}

fn validate_composition_value(value: &crate::CompositionValue) -> Result<(), CalculationError> {
    let invalid_status = value.status == ValueStatus::Missing && value.amount.is_some();
    let invalid_amount = value.amount.is_some_and(|amount| amount < Decimal::ZERO);
    let invalid_lower = value.lower_amount.is_some_and(|lower| {
        lower < Decimal::ZERO || value.amount.is_some_and(|amount| lower > amount)
    });
    let invalid_upper = value
        .upper_amount
        .is_some_and(|upper| value.amount.is_some_and(|amount| upper < amount));
    if invalid_status || invalid_amount || invalid_lower || invalid_upper {
        return Err(CalculationError::InvalidCompositionValue(
            value.nutrient.to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CompositionSnapshot, CompositionValue, EvidenceQuality, FoodId, MassEstimate,
        MassResolutionMethod, ValueStatus,
    };
    use rust_decimal::Decimal;

    fn decimal(value: i64) -> Decimal {
        Decimal::from(value)
    }

    fn code(value: &str) -> NutrientCode {
        NutrientCode::new(value).expect("valid fixture nutrient code")
    }

    fn direct_item(mass: i64) -> ResolvedItemInput {
        ResolvedItemInput {
            food_id: FoodId::new(),
            mass: MassEstimate {
                central_g: decimal(mass),
                lower_g: None,
                upper_g: None,
                evidence_id: None,
                method: MassResolutionMethod::ExplicitMass,
            },
            composition: CompositionSnapshot {
                profile_id: CompositionProfileId::new(),
                basis_g: decimal(100),
                quality: EvidenceQuality::A,
                values: vec![
                    CompositionValue {
                        nutrient: code("energy_kcal"),
                        amount: Some(decimal(155)),
                        lower_amount: None,
                        upper_amount: None,
                        unit: NutrientUnit::Kilocalorie,
                        status: ValueStatus::Measured,
                    },
                    CompositionValue {
                        nutrient: code("protein_g"),
                        amount: Some(Decimal::new(126, 1)),
                        lower_amount: None,
                        upper_amount: None,
                        unit: NutrientUnit::Gram,
                        status: ValueStatus::Measured,
                    },
                ],
            },
            recipe_version_id: None,
        }
    }

    #[test]
    fn direct_profile_scales_by_mass() {
        let result = DeterministicCalculator::calculate(&CalculationInput {
            engine_version: "calc-test".to_owned(),
            requested_nutrients: vec![code("energy_kcal"), code("protein_g")],
            items: vec![direct_item(50)],
        })
        .expect("fixture should calculate");

        assert_eq!(result.totals[0].amount, Some(Decimal::new(775, 1)));
        assert_eq!(result.totals[0].completeness_ratio, Decimal::ONE);
        assert_eq!(result.totals[1].amount, Some(Decimal::new(63, 1)));
    }

    #[test]
    fn missing_is_not_zero_and_reduces_completeness() {
        let mut missing_item = direct_item(50);
        missing_item.composition.values.clear();
        let result = DeterministicCalculator::calculate(&CalculationInput {
            engine_version: "calc-test".to_owned(),
            requested_nutrients: vec![code("energy_kcal")],
            items: vec![direct_item(50), missing_item],
        })
        .expect("partial total is valid");

        assert_eq!(result.totals[0].amount, Some(Decimal::new(775, 1)));
        assert_eq!(result.totals[0].completeness_ratio, Decimal::new(5, 1));
        assert_eq!(result.items[1].nutrients[0].amount, None);
    }

    #[test]
    fn bounds_contain_central_result() {
        let mut item = direct_item(100);
        item.mass.lower_g = Some(decimal(80));
        item.mass.upper_g = Some(decimal(120));
        let result = DeterministicCalculator::calculate(&CalculationInput {
            engine_version: "calc-test".to_owned(),
            requested_nutrients: vec![code("energy_kcal")],
            items: vec![item],
        })
        .expect("bounded fixture should calculate");
        let total = &result.totals[0];

        assert_eq!(total.lower_amount, Some(decimal(124)));
        assert_eq!(total.amount, Some(decimal(155)));
        assert_eq!(total.upper_amount, Some(decimal(186)));
    }

    #[test]
    fn rejects_duplicate_nutrients() {
        let mut item = direct_item(100);
        item.composition
            .values
            .push(item.composition.values[0].clone());
        let error = DeterministicCalculator::calculate(&CalculationInput {
            engine_version: "calc-test".to_owned(),
            requested_nutrients: vec![code("energy_kcal")],
            items: vec![item],
        })
        .expect_err("duplicate nutrient must fail");

        assert_eq!(
            error,
            CalculationError::DuplicateNutrient("energy_kcal".to_owned())
        );
    }
}
