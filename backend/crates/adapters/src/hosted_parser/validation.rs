//! Schema and semantic parser output validation responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn validate_parse_request(request: &ParseRequest) -> Result<(), ApplicationError> {
    if request.text.trim().is_empty()
        || request.text.len() > 16 * 1_024
        || request.locale.trim().is_empty()
        || request.locale.len() > 32
    {
        return Err(ApplicationError::InvalidInput(
            "meal parser input is outside configured bounds".to_owned(),
        ));
    }
    Ok(())
}

pub(crate) enum OutputFailure {
    Schema,
    Semantic(String),
}

pub(crate) fn validate_output(
    request: &ParseRequest,
    output: Value,
) -> Result<ParsedMealDocument, OutputFailure> {
    let validator = jsonschema::validator_for(
        &serde_json::from_str(PARSER_SCHEMA).map_err(|_| OutputFailure::Schema)?,
    )
    .map_err(|_| OutputFailure::Schema)?;
    validator
        .validate(&output)
        .map_err(|_| OutputFailure::Schema)?;
    let mut document: ParsedMealDocument =
        serde_json::from_value(output).map_err(|_| OutputFailure::Schema)?;
    let expected_language = request
        .locale
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    if document.language.to_lowercase() != expected_language {
        return Err(OutputFailure::Semantic(
            "provider_semantic_validation_failed".to_owned(),
        ));
    }
    let mut unique_items = BTreeSet::new();
    for item in &mut document.items {
        let normalized_source = normalize_vi_search_key(&item.source_text);
        let normalized_food = normalize_vi_search_key(&item.food_phrase);
        let modifiers_are_grounded = item.modifiers.iter().all(|modifier| {
            let normalized = normalize_vi_search_key(modifier);
            !normalized.is_empty() && normalized_source.contains(&normalized)
        });
        let unit_is_grounded = item.unit_phrase.as_deref().is_none_or(|unit| {
            let normalized = normalize_vi_search_key(unit);
            !normalized.is_empty() && normalized_source.contains(&normalized)
        });
        if !request.text.contains(&item.source_text)
            || normalized_food.is_empty()
            || !normalized_source.contains(&normalized_food)
            || !modifiers_are_grounded
            || !unit_is_grounded
            || contains_negated_consumption(&item.source_text)
            || !unique_items.insert((normalized_source, normalized_food))
        {
            return Err(OutputFailure::Semantic(
                "provider_semantic_validation_failed".to_owned(),
            ));
        }
        item.unit_phrase = item.unit_phrase.as_deref().map(normalize_vi_search_key);
    }
    if suspicious_instruction(&request.text)
        && document.warnings.len() < 20
        && !document
            .warnings
            .iter()
            .any(|warning| warning == "suspicious_instruction_text")
    {
        document
            .warnings
            .push("suspicious_instruction_text".to_owned());
    }
    Ok(document)
}

pub(crate) fn contains_negated_consumption(value: &str) -> bool {
    let normalized = normalize_vi_search_key(value);
    normalized.contains("không ăn")
        || normalized.contains("không uống")
        || normalized.contains("khong an")
        || normalized.contains("khong uong")
}

pub(crate) fn suspicious_instruction(value: &str) -> bool {
    let normalized = value.to_lowercase();
    normalized.contains("ignore previous")
        || normalized.contains("bỏ qua hướng dẫn")
        || normalized.contains("system prompt")
}
