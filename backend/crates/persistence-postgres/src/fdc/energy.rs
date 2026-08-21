#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn source_energy_summary(foods: &[RawFood], errors: &mut Vec<String>) -> EnergySummary {
    let mut summary = EnergySummary::default();
    for food in foods {
        match extract_energy(food) {
            Ok(energy) => add_energy_summary(&mut summary, &energy),
            Err(error) => errors.push(format!(
                "FDC ID {} energy validation failed: {error}",
                food.fdc_id
            )),
        }
    }
    summary
}

pub(crate) fn add_energy_summary(summary: &mut EnergySummary, energy: &EnergyExtraction) {
    match energy
        .selected
        .as_ref()
        .map(|value| value.source_nutrient_id)
    {
        Some(2048) => summary.atwater_specific += 1,
        Some(2047) => summary.atwater_general += 1,
        None => summary.missing_energy += 1,
        Some(_) => unreachable!("energy extraction only selects 2048 or 2047"),
    }
    summary.unexpected_legacy += energy.unexpected_legacy_count;
}

pub(crate) fn extract_energy(food: &RawFood) -> Result<EnergyExtraction, FdcFoundationImportError> {
    let food_nutrients = food
        .payload
        .get("foodNutrients")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            FdcFoundationImportError::InvalidInput(format!(
                "FDC ID {} has no foodNutrients array",
                food.fdc_id
            ))
        })?;

    let specific = extract_energy_candidate(food, food_nutrients, 2048, "atwater_specific")?;
    let general = extract_energy_candidate(food, food_nutrients, 2047, "atwater_general")?;
    let unexpected_legacy_count = food_nutrients
        .iter()
        .filter(|item| {
            item.get("nutrient")
                .and_then(|nutrient| nutrient.get("id"))
                .and_then(Value::as_u64)
                == Some(1008)
        })
        .count();

    Ok(EnergyExtraction {
        selected: specific.or(general),
        unexpected_legacy_count,
    })
}

pub(crate) fn extract_energy_candidate(
    food: &RawFood,
    food_nutrients: &[Value],
    source_nutrient_id: u64,
    source_method: &'static str,
) -> Result<Option<StagedNutrient>, FdcFoundationImportError> {
    let matches = food_nutrients
        .iter()
        .filter(|item| {
            item.get("nutrient")
                .and_then(|nutrient| nutrient.get("id"))
                .and_then(Value::as_u64)
                == Some(source_nutrient_id)
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {} contains duplicate energy nutrient {}",
            food.fdc_id, source_nutrient_id
        )));
    }
    let Some(item) = matches.first().copied() else {
        return Ok(None);
    };
    staged_nutrient_from_item(
        food,
        item,
        source_nutrient_id,
        "energy_kcal",
        "kcal",
        Some(source_method),
    )
    .map(Some)
}
