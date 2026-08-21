#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn extract_unambiguous_macronutrients(
    food: &RawFood,
) -> Result<Vec<StagedNutrient>, FdcFoundationImportError> {
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

    let definitions = [
        (1003_u64, "protein_g", "g"),
        (1004_u64, "fat_g", "g"),
        (1005_u64, "carbohydrate_g", "g"),
    ];
    let mut values = Vec::with_capacity(definitions.len());
    for (source_nutrient_id, internal_code, expected_unit) in definitions {
        values.push(extract_macronutrient(
            food,
            food_nutrients,
            source_nutrient_id,
            internal_code,
            expected_unit,
        )?);
    }
    Ok(values)
}

pub(crate) fn extract_macronutrient(
    food: &RawFood,
    food_nutrients: &[Value],
    source_nutrient_id: u64,
    internal_code: &'static str,
    expected_unit: &str,
) -> Result<StagedNutrient, FdcFoundationImportError> {
    let matches = food_nutrients
        .iter()
        .filter(|item| {
            item.get("nutrient")
                .and_then(|nutrient| nutrient.get("id"))
                .and_then(Value::as_u64)
                == Some(source_nutrient_id)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {} must contain exactly one source nutrient {} for {}",
            food.fdc_id, source_nutrient_id, internal_code
        )));
    }
    let item = matches[0];
    staged_nutrient_from_item(
        food,
        item,
        source_nutrient_id,
        internal_code,
        expected_unit,
        None,
    )
}

pub(crate) fn staged_nutrient_from_item(
    food: &RawFood,
    item: &Value,
    source_nutrient_id: u64,
    internal_code: &'static str,
    expected_unit: &str,
    source_method: Option<&'static str>,
) -> Result<StagedNutrient, FdcFoundationImportError> {
    validate_nutrient_unit(food.fdc_id, source_nutrient_id, item, expected_unit)?;
    let amount = required_nonnegative_amount(food.fdc_id, source_nutrient_id, item)?;
    let minimum = decimal_field(item, "min")?;
    let maximum = decimal_field(item, "max")?;
    if let (Some(minimum), Some(maximum)) = (minimum, maximum)
        && minimum > maximum
    {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {} nutrient {} has min greater than max",
            food.fdc_id, source_nutrient_id
        )));
    }
    let method_code = item
        .get("foodNutrientDerivation")
        .and_then(|derivation| derivation.get("code"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(StagedNutrient {
        internal_code,
        source_nutrient_id,
        source_method,
        amount,
        minimum,
        maximum,
        method_code,
    })
}

pub(crate) fn validate_nutrient_unit(
    fdc_id: u64,
    source_nutrient_id: u64,
    item: &Value,
    expected_unit: &str,
) -> Result<(), FdcFoundationImportError> {
    let unit = item
        .get("nutrient")
        .and_then(|nutrient| nutrient.get("unitName"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            FdcFoundationImportError::InvalidInput(format!(
                "FDC ID {fdc_id} nutrient {source_nutrient_id} has no unitName"
            ))
        })?;
    if !unit.eq_ignore_ascii_case(expected_unit) {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {fdc_id} nutrient {source_nutrient_id} uses unit {unit}, expected {expected_unit}"
        )));
    }
    Ok(())
}

pub(crate) fn required_nonnegative_amount(
    fdc_id: u64,
    source_nutrient_id: u64,
    item: &Value,
) -> Result<Decimal, FdcFoundationImportError> {
    let amount = decimal_field(item, "amount")?.ok_or_else(|| {
        FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {fdc_id} nutrient {source_nutrient_id} has no amount"
        ))
    })?;
    if amount.is_sign_negative() {
        return Err(FdcFoundationImportError::InvalidInput(format!(
            "FDC ID {fdc_id} nutrient {source_nutrient_id} has a negative amount"
        )));
    }
    Ok(amount)
}

pub(crate) fn decimal_field(
    value: &Value,
    field: &str,
) -> Result<Option<Decimal>, FdcFoundationImportError> {
    let Some(raw) = value.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let text = match raw {
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        _ => {
            return Err(FdcFoundationImportError::InvalidInput(format!(
                "{field} must be numeric"
            )));
        }
    };
    Decimal::from_str(&text)
        .map(Some)
        .map_err(|_| FdcFoundationImportError::InvalidInput(format!("{field} is not a decimal")))
}
