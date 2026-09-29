from __future__ import annotations

import hashlib
import json
from typing import Any

from .curation.reviewed_mapping import DECISION_REFERENCE, REVIEWED_MAPPING_VERSION, SOURCE_RECORD


MANIFEST_SCHEMA_VERSION = "portion-study-manifest-0.3.0"
MANIFEST_POLICY_VERSION = "portion-study-0.3.0"
PLANNING_PACKET_SCHEMA_VERSION = "portion-planning-packet-0.1.0"
PLANNING_PACKET_POLICY_VERSION = "portion-planning-0.1.0"
REVIEWED_MAPPING_REF = (
    "data-factory/docs/reviews/"
    "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json"
)
REVIEWED_MAPPING_SHA256 = "c7ab626ed66c01a84c6b00d7e312a3e061eef4af2ec4dcfed950e6b398307324"
TARGET = "cơm trắng"
SPECIFICITY_POLICY_REF = "resolve-exact-specificity-0.2.0"
PLANNING_MANIFEST_REF = "data-factory/config/portion-study-com-trang-bat-0.3.0.json"

UNRESOLVED_HUMAN_INPUTS = (
    ("physical_definition_context_of_bat", 'Physical definition/context of "bát"'),
    ("vessel_context_sampling_strategy", "Vessel/context sampling strategy"),
    ("minimum_independent_sample_count", "Minimum independent sample count"),
    ("minimum_batch_count", "Minimum batch count"),
    ("instrument", "Measurement instrument"),
    ("calibration", "Instrument calibration/check data"),
    ("tare", "Tare method and tare mass"),
    ("operators", "Measurement operators"),
    ("measurement_dates", "Measurement dates"),
    ("measurement_reviewer", "Measurement reviewer and review reference"),
    ("actual_masses", "Actual measured masses"),
)


def validate_portion_planning_manifest(
    value: Any, reviewed_mapping_bytes: bytes
) -> tuple[dict[str, Any] | None, tuple[dict[str, str], ...]]:
    errors: list[dict[str, str]] = []
    if not isinstance(value, dict):
        return None, ({"reason_code": "manifest_not_object"},)

    try:
        mapping = json.loads(reviewed_mapping_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError):
        mapping = None
    if hashlib.sha256(reviewed_mapping_bytes).hexdigest() != REVIEWED_MAPPING_SHA256:
        errors.append({"reason_code": "reviewed_mapping_hash_mismatch"})
    if not _reviewed_mapping_matches(mapping):
        errors.append({"reason_code": "reviewed_mapping_identity_mismatch"})

    _check_exact_keys(
        value,
        {"schema_version", "study_id", "policy_version", "target", "unresolved_human_inputs"},
        "manifest",
        errors,
    )
    _require_equal(value, "schema_version", MANIFEST_SCHEMA_VERSION, "manifest", errors)
    _require_equal(value, "policy_version", MANIFEST_POLICY_VERSION, "manifest", errors)
    if not isinstance(value.get("study_id"), str) or not value["study_id"].strip():
        errors.append({"reason_code": "invalid_study_id"})

    target = value.get("target")
    if not isinstance(target, dict):
        errors.append({"reason_code": "target_not_object"})
    else:
        _check_exact_keys(target, {"normalized_vietnamese_target", "identity", "preparation", "measure"}, "target", errors)
        _require_equal(target, "normalized_vietnamese_target", TARGET, "target", errors)

        identity = target.get("identity")
        if not isinstance(identity, dict):
            errors.append({"reason_code": "identity_not_object"})
        else:
            _check_exact_keys(
                identity,
                {"status", "reviewed_evidence_identity", "backend_food_id", "reviewer"},
                "target.identity",
                errors,
            )
            _require_equal(identity, "status", "reviewed_evidence", "target.identity", errors)
            if identity.get("backend_food_id") is not None:
                errors.append({"reason_code": "backend_food_id_not_assigned_for_reviewed_evidence"})
            _require_equal(identity, "reviewer", "pumni", "target.identity", errors)
            evidence_identity = identity.get("reviewed_evidence_identity")
            if not _same_typed_json(evidence_identity, _expected_evidence_identity()):
                errors.append({"reason_code": "reviewed_evidence_identity_mismatch"})

        preparation = target.get("preparation")
        expected_definition = _mapping_semantic_boundary(mapping)
        if not isinstance(preparation, dict):
            errors.append({"reason_code": "preparation_not_object"})
        else:
            _check_exact_keys(preparation, {"status", "definition", "reviewer", "review_ref"}, "target.preparation", errors)
            _require_equal(preparation, "status", "reviewed", "target.preparation", errors)
            _require_equal(preparation, "definition", expected_definition, "target.preparation", errors)
            _require_equal(preparation, "reviewer", "pumni", "target.preparation", errors)
            _require_equal(preparation, "review_ref", DECISION_REFERENCE, "target.preparation", errors)

        measure = target.get("measure")
        if not isinstance(measure, dict):
            errors.append({"reason_code": "measure_not_object"})
        else:
            _check_exact_keys(
                measure,
                {
                    "canonical_measure",
                    "original_unit_phrase",
                    "represented_quantity",
                    "context_status",
                    "physical_context",
                    "context_reviewer",
                    "context_review_ref",
                },
                "target.measure",
                errors,
            )
            for key, expected in (
                ("canonical_measure", "bát"),
                ("original_unit_phrase", "bát"),
                ("represented_quantity", 1),
                ("context_status", "unresolved"),
                ("physical_context", None),
                ("context_reviewer", None),
                ("context_review_ref", None),
            ):
                _require_equal(measure, key, expected, "target.measure", errors)

    human_inputs = value.get("unresolved_human_inputs")
    if not isinstance(human_inputs, dict):
        errors.append({"reason_code": "unresolved_human_inputs_not_object"})
    else:
        expected_keys = {key for key, _ in UNRESOLVED_HUMAN_INPUTS}
        _check_exact_keys(human_inputs, expected_keys, "unresolved_human_inputs", errors)
        for key in sorted(expected_keys):
            if human_inputs.get(key) is not None:
                errors.append({"reason_code": "human_input_must_remain_unresolved", "field": key})

    if errors:
        return None, tuple(errors)
    return value, ()


def build_portion_planning_packet(
    manifest_bytes: bytes, reviewed_mapping_bytes: bytes
) -> tuple[dict[str, Any] | None, tuple[dict[str, str], ...]]:
    try:
        manifest_value = json.loads(manifest_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None, ({"reason_code": "manifest_not_json"},)
    manifest, errors = validate_portion_planning_manifest(manifest_value, reviewed_mapping_bytes)
    if manifest is None:
        return None, errors

    target = manifest["target"]
    measure = target["measure"]
    mapping = json.loads(reviewed_mapping_bytes)
    return (
        {
            "schema_version": PLANNING_PACKET_SCHEMA_VERSION,
            "policy_version": PLANNING_PACKET_POLICY_VERSION,
            "status": "blocked_unresolved_human_inputs",
            "study_id": manifest["study_id"],
            "input_artifacts": {
                "manifest": {
                    "artifact_ref": PLANNING_MANIFEST_REF,
                    "sha256": hashlib.sha256(manifest_bytes).hexdigest(),
                },
                "reviewed_mapping": {
                    "artifact_ref": REVIEWED_MAPPING_REF,
                    "sha256": REVIEWED_MAPPING_SHA256,
                },
            },
            "target": {
                "normalized_vietnamese_target": target["normalized_vietnamese_target"],
                "reviewed_evidence_identity": target["identity"]["reviewed_evidence_identity"],
                "backend_food_id": target["identity"]["backend_food_id"],
                "preparation": target["preparation"]["definition"],
                "measure": {
                    "canonical_measure": measure["canonical_measure"],
                    "original_unit_phrase": measure["original_unit_phrase"],
                    "represented_quantity": measure["represented_quantity"],
                    "context_status": measure["context_status"],
                    "physical_context": measure["physical_context"],
                },
            },
            "unresolved_human_inputs": [
                {"key": key, "description": description, "status": "unresolved"}
                for key, description in UNRESOLVED_HUMAN_INPUTS
            ],
            "readiness": {
                "ready_for_measurement": False,
                "review_ready": False,
                "blocker_count": len(UNRESOLVED_HUMAN_INPUTS),
            },
            "measurement_state": {"status": "not_collected", "observation_count": 0},
            "mass_estimate": None,
            "gram_estimate_emitted": False,
            "publication": {
                "status": "blocked",
                "publishable": False,
                "release_created": False,
                "activation_attempted": False,
            },
        },
        (),
    )


def canonical_json_bytes(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def _expected_evidence_identity() -> dict[str, Any]:
    return {
        "artifact_ref": REVIEWED_MAPPING_REF,
        "schema_version": REVIEWED_MAPPING_VERSION,
        "sha256": REVIEWED_MAPPING_SHA256,
        "normalized_vietnamese_target": TARGET,
        "decision_ref": DECISION_REFERENCE,
        "source": {
            "source_code": SOURCE_RECORD["source_code"],
            "release": SOURCE_RECORD["release"],
            "fndds_food_code": SOURCE_RECORD["fndds_food_code"],
            "fdc_id": SOURCE_RECORD["fdc_id"],
            "source_record_sha256": SOURCE_RECORD["record_sha256"],
        },
        "specificity_policy_ref": SPECIFICITY_POLICY_REF,
    }


def _reviewed_mapping_matches(value: Any) -> bool:
    if not isinstance(value, dict):
        return False
    mapping = value.get("mapping")
    source_record = mapping.get("source_record") if isinstance(mapping, dict) else None
    decision = value.get("review_decision")
    return (
        value.get("schema_version") == REVIEWED_MAPPING_VERSION
        and isinstance(mapping, dict)
        and mapping.get("normalized_vietnamese_target") == TARGET
        and mapping.get("identity_status") == "reviewed_supported"
        and mapping.get("composition_status") == "reviewed_supported"
        and mapping.get("runtime_specificity_policy_reference") == SPECIFICITY_POLICY_REF
        and isinstance(source_record, dict)
        and source_record.get("source_code") == SOURCE_RECORD["source_code"]
        and source_record.get("release") == SOURCE_RECORD["release"]
        and source_record.get("fndds_food_code") == SOURCE_RECORD["fndds_food_code"]
        and source_record.get("fdc_id") == SOURCE_RECORD["fdc_id"]
        and source_record.get("record_sha256") == SOURCE_RECORD["record_sha256"]
        and isinstance(decision, dict)
        and decision.get("decision") == "approved"
        and decision.get("decision_reference") == DECISION_REFERENCE
    )


def _mapping_semantic_boundary(mapping: Any) -> str | None:
    if not isinstance(mapping, dict):
        return None
    reviewed_mapping = mapping.get("mapping")
    if not isinstance(reviewed_mapping, dict):
        return None
    value = reviewed_mapping.get("semantic_boundary")
    return value if isinstance(value, str) else None


def _check_exact_keys(
    value: dict[str, Any], expected: set[str], path: str, errors: list[dict[str, str]]
) -> None:
    for key in sorted(expected - value.keys()):
        errors.append({"reason_code": "missing_field", "field": f"{path}.{key}"})
    for key in sorted(value.keys() - expected):
        errors.append({"reason_code": "unknown_field", "field": f"{path}.{key}"})


def _require_equal(
    value: dict[str, Any], field: str, expected: Any, path: str, errors: list[dict[str, str]]
) -> None:
    actual = value.get(field)
    if type(actual) is not type(expected) or actual != expected:
        errors.append({"reason_code": "pinned_value_mismatch", "field": f"{path}.{field}"})


def _same_typed_json(actual: Any, expected: Any) -> bool:
    if type(actual) is not type(expected):
        return False
    if isinstance(expected, dict):
        return actual.keys() == expected.keys() and all(
            _same_typed_json(actual[key], expected[key]) for key in expected
        )
    if isinstance(expected, list):
        return len(actual) == len(expected) and all(
            _same_typed_json(left, right) for left, right in zip(actual, expected, strict=True)
        )
    return actual == expected
