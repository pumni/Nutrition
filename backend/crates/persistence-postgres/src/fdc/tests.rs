use super::{
    FdcFoundationValidationRequest, build_fdc_selection_candidate_manifest, candidate_from_food,
    extract_energy, extract_unambiguous_macronutrients, parse_source_foods, reviewed_selection,
    transform_fdc_foundation_2026_04_null_tail, validate_fdc_foundation_json,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MINIMAL: &str = r#"{
      "FoundationFoods": [{
        "fdcId": 900000001,
        "dataType": "Foundation",
        "description": "Synthetic foundation food",
        "foodNutrients": [
          {"amount": 2.5, "nutrient": {"id": 1003, "unitName": "G"}},
          {"amount": 3.5, "nutrient": {"id": 1004, "unitName": "G"}},
          {"amount": 4.5, "nutrient": {"id": 1005, "unitName": "G"}},
          {"amount": 99, "nutrient": {"id": 2048, "unitName": "KCAL"}}
        ]
      }]
    }"#;

#[test]
fn parses_foundation_shape_and_preserves_specific_energy_provenance() {
    let foods = parse_source_foods(MINIMAL.as_bytes()).expect("fixture must parse");
    let nutrients = extract_unambiguous_macronutrients(&foods[0])
        .expect("unambiguous macro nutrients must map");
    let codes = nutrients
        .iter()
        .map(|nutrient| nutrient.internal_code)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        codes,
        BTreeSet::from(["carbohydrate_g", "fat_g", "protein_g"])
    );
    let energy = extract_energy(&foods[0])
        .expect("specific energy must parse")
        .selected
        .expect("specific energy must be selected");
    assert_eq!(energy.internal_code, "energy_kcal");
    assert_eq!(energy.source_nutrient_id, 2048);
    assert_eq!(energy.source_method, Some("atwater_specific"));
    assert_eq!(energy.amount.to_string(), "99");
}

#[test]
fn energy_policy_prefers_specific_and_falls_back_to_general() {
    let both = energy_food(&json!([
        nutrient(2048, "KCAL", &json!(52)),
        nutrient(2047, "KCAL", &json!(53))
    ]));
    let specific = extract_energy(&both)
        .expect("valid energy candidates must parse")
        .selected
        .expect("specific energy must win");
    assert_eq!(specific.source_nutrient_id, 2048);
    assert_eq!(specific.source_method, Some("atwater_specific"));

    let general = energy_food(&json!([nutrient(2047, "KCAL", &json!(53))]));
    let general = extract_energy(&general)
        .expect("general energy must parse")
        .selected
        .expect("general energy must be selected");
    assert_eq!(general.source_nutrient_id, 2047);
    assert_eq!(general.source_method, Some("atwater_general"));
}

#[test]
fn energy_policy_marks_missing_and_legacy_values_incomplete() {
    let legacy = extract_energy(&energy_food(&json!([nutrient(1008, "KCAL", &json!(50))])))
        .expect("legacy energy is reportable");
    assert!(legacy.selected.is_none());
    assert_eq!(legacy.unexpected_legacy_count, 1);

    let missing = extract_energy(&energy_food(&json!([]))).expect("missing energy is reportable");
    assert!(missing.selected.is_none());
    assert_eq!(missing.unexpected_legacy_count, 0);
}

#[test]
fn energy_policy_fails_closed_on_malformed_or_duplicate_candidates() {
    let malformed_specific = energy_food(&json!([
        nutrient(2048, "KCAL", &json!("malformed")),
        nutrient(2047, "KCAL", &json!(53))
    ]));
    assert!(extract_energy(&malformed_specific).is_err());

    let duplicate_specific = energy_food(&json!([
        nutrient(2048, "KCAL", &json!(52)),
        nutrient(2048, "KCAL", &json!(53))
    ]));
    assert!(extract_energy(&duplicate_specific).is_err());

    let invalid_unit = energy_food(&json!([nutrient(2048, "G", &json!(52))]));
    assert!(extract_energy(&invalid_unit).is_err());
}

#[test]
fn selection_candidates_require_complete_macros_and_non_legacy_energy() {
    let eligible = candidate_from_food(&candidate_food(
        900_000_001,
        &json!([nutrient(2048, "KCAL", &json!(52))]),
    ))
    .expect("complete candidate must be eligible");
    assert_eq!(eligible.fdc_id, 900_000_001);
    assert_eq!(eligible.energy_source_nutrient_id, 2048);
    assert_eq!(eligible.energy_method, "atwater_specific");

    let legacy = candidate_from_food(&candidate_food(
        900_000_002,
        &json!([nutrient(1008, "KCAL", &json!(50))]),
    ));
    assert!(legacy.is_none());

    let incomplete =
        candidate_from_food(&energy_food(&json!([nutrient(2048, "KCAL", &json!(52))])));
    assert!(incomplete.is_none());
}

#[test]
fn candidate_manifest_rejects_an_unpinned_artifact() {
    let request = validation_request(MINIMAL, Vec::new());
    let error = build_fdc_selection_candidate_manifest(MINIMAL.as_bytes(), &request, 20)
        .expect_err("candidate evidence must require the pinned release contract");
    assert!(error.to_string().contains("preprocessing"));
}

fn nutrient(id: u64, unit: &str, amount: &Value) -> Value {
    json!({
        "amount": amount,
        "nutrient": {"id": id, "unitName": unit}
    })
}

fn energy_food(food_nutrients: &Value) -> super::RawFood {
    let payload = json!({
        "FoundationFoods": [{
            "fdcId": 900_000_099,
            "dataType": "Foundation",
            "description": "Synthetic energy policy food",
            "foodNutrients": food_nutrients
        }]
    });
    let bytes = serde_json::to_vec(&payload).expect("synthetic energy JSON must serialize");
    parse_source_foods(&bytes)
        .expect("synthetic energy food must parse")
        .remove(0)
}

fn candidate_food(fdc_id: u64, energy_nutrients: &Value) -> super::RawFood {
    let mut food_nutrients = vec![
        nutrient(1003, "G", &json!(2.5)),
        nutrient(1004, "G", &json!(3.5)),
        nutrient(1005, "G", &json!(4.5)),
    ];
    food_nutrients.extend(
        energy_nutrients
            .as_array()
            .expect("energy fixture must be an array")
            .iter()
            .cloned(),
    );
    let payload = json!({
        "FoundationFoods": [{
            "fdcId": fdc_id,
            "dataType": "Foundation",
            "description": "Synthetic selection candidate",
            "foodNutrients": food_nutrients
        }]
    });
    let bytes = serde_json::to_vec(&payload).expect("synthetic candidate JSON must serialize");
    parse_source_foods(&bytes)
        .expect("synthetic candidate food must parse")
        .remove(0)
}

#[test]
fn reviewed_selection_rejects_empty_and_duplicate_ids() {
    assert!(reviewed_selection(&[]).is_err());
    assert!(reviewed_selection(&[1, 1]).is_err());
    assert_eq!(
        reviewed_selection(&[2, 1]).expect("unique selection").len(),
        2
    );
}

#[test]
fn validation_report_is_deterministic_and_preserves_selection_energy_counts() {
    let request = validation_request(MINIMAL, vec![900_000_001]);
    let first = validate_fdc_foundation_json(MINIMAL.as_bytes(), &request);
    let second = validate_fdc_foundation_json(MINIMAL.as_bytes(), &request);

    assert_eq!(first, second);
    assert_eq!(first.artifact_status, "valid");
    assert_eq!(first.selection_status, "validated");
    assert_eq!(first.validation_status, "passed");
    assert_eq!(first.raw_record_count, 1);
    assert_eq!(first.selected_record_count, 1);
    assert_eq!(first.source_energy_atwater_specific_count, 1);
    assert_eq!(first.selected_energy_atwater_specific_count, 1);
    assert_eq!(
        first
            .to_pretty_json(&request)
            .expect("validation report must serialize"),
        second
            .to_pretty_json(&request)
            .expect("validation report must serialize")
    );
    assert_eq!(
        first
            .to_json(&request)
            .get("production_eligible")
            .and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn validation_report_fails_closed_on_null_records_and_unapproved_selection() {
    let source = r#"{
          "FoundationFoods": [
            {
              "fdcId": 900000001,
              "dataType": "Foundation",
              "description": "Synthetic foundation food",
              "foodNutrients": [
                {"amount": 2.5, "nutrient": {"id": 1003, "unitName": "G"}},
                {"amount": 3.5, "nutrient": {"id": 1004, "unitName": "G"}},
                {"amount": 4.5, "nutrient": {"id": 1005, "unitName": "G"}},
                {"amount": 99, "nutrient": {"id": 2048, "unitName": "KCAL"}}
              ]
            },
            null
          ]
        }"#;
    let request = validation_request(source, Vec::new());
    let report = validate_fdc_foundation_json(source.as_bytes(), &request);

    assert_eq!(report.raw_record_count, 2);
    assert_eq!(report.valid_record_count, 1);
    assert_eq!(report.null_record_count, 1);
    assert_eq!(report.invalid_record_count, 1);
    assert_eq!(report.artifact_status, "invalid");
    assert_eq!(report.selection_status, "not_approved");
    assert_eq!(report.validation_status, "blocked");
    assert!(report.errors.iter().any(|error| error.contains("is null")));
    assert!(
        report
            .errors
            .iter()
            .any(|error| error.contains("not approved"))
    );
}

#[test]
fn null_tail_preprocessing_removes_only_the_approved_tail() {
    let mut records = (0..363)
        .map(|index| json!({"fdcId": index, "marker": format!("record-{index}")}))
        .collect::<Vec<_>>();
    records.extend(std::iter::repeat_n(Value::Null, 32));
    let mut root = json!({"FoundationFoods": records});

    let normalized = transform_fdc_foundation_2026_04_null_tail(&mut root)
        .expect("the exact 32-null tail must be transformable");
    let normalized_root: Value =
        serde_json::from_slice(&normalized).expect("normalized payload must be JSON");
    let foods = normalized_root
        .get("FoundationFoods")
        .and_then(Value::as_array)
        .expect("normalized payload must retain FoundationFoods");
    assert_eq!(foods.len(), 363);
    assert_eq!(foods[0]["marker"], "record-0");
    assert_eq!(foods[362]["marker"], "record-362");
}

#[test]
fn null_tail_preprocessing_rejects_interior_or_non_null_tail_entries() {
    let mut interior_null_records = (0..363)
        .map(|index| json!({"fdcId": index}))
        .collect::<Vec<_>>();
    interior_null_records[12] = Value::Null;
    interior_null_records.extend(std::iter::repeat_n(Value::Null, 32));
    let mut interior_null_root = json!({"FoundationFoods": interior_null_records});
    let interior_error = transform_fdc_foundation_2026_04_null_tail(&mut interior_null_root)
        .expect_err("interior nulls must fail closed");
    assert!(interior_error.contains("interior null"));

    let mut non_null_tail_records = (0..363)
        .map(|index| json!({"fdcId": index}))
        .collect::<Vec<_>>();
    non_null_tail_records.extend(std::iter::repeat_n(Value::Null, 31));
    non_null_tail_records.push(json!({"fdcId": 999}));
    let mut non_null_tail_root = json!({"FoundationFoods": non_null_tail_records});
    let tail_error = transform_fdc_foundation_2026_04_null_tail(&mut non_null_tail_root)
        .expect_err("a non-null tail entry must fail closed");
    assert!(tail_error.contains("exactly 32 null entries"));
}

#[test]
fn requested_preprocessing_rejects_unpinned_source_hashes() {
    let source = MINIMAL;
    let mut request = validation_request(source, Vec::new());
    request.preprocessing_policy_version =
        Some(super::FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION.to_owned());
    let report = validate_fdc_foundation_json(source.as_bytes(), &request);

    assert!(!report.source_integrity_valid.is_valid());
    assert!(!report.preprocessing_applied.is_applied());
    assert!(!report.normalized_payload_valid.is_valid());
    assert!(
        report
            .errors
            .iter()
            .any(|error| { error.contains("requires extracted JSON SHA-256") })
    );
}

fn validation_request(source: &str, reviewed_fdc_ids: Vec<u64>) -> FdcFoundationValidationRequest {
    FdcFoundationValidationRequest {
        release_version: "2026-04-30".to_owned(),
        source_published_date: "2026-04-30".to_owned(),
        object_uri: "fixture://fdc/validation.json".to_owned(),
        source_payload_filename: None,
        source_archive_sha256: None,
        expected_sha256: super::sha256_hex(source.as_bytes()),
        reviewed_fdc_ids,
        preprocessing_policy_version: None,
    }
}
