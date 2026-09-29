from __future__ import annotations

import hashlib
import json
import re
from typing import Any

from .portion import BOUND_POLICY_VERSION, ESTIMATOR_VERSION, PORTION_PROTOCOL_VERSION
from .portion_planning import (
    PLANNING_PACKET_SCHEMA_VERSION,
    PLANNING_MANIFEST_REF,
    REVIEWED_MAPPING_REF,
    REVIEWED_MAPPING_SHA256,
    build_portion_planning_packet,
    canonical_json_bytes,
    validate_portion_planning_manifest,
)
from .curation.reviewed_mapping import DECISION_REFERENCE, SOURCE_RECORD


STRATEGY_DECISION_SCHEMA_VERSION = "portion-study-strategy-decision-0.1.0"
STUDY_DEFINITION_SCHEMA_VERSION = "portion-study-definition-0.1.0"
STUDY_DEFINITION_POLICY_VERSION = "portion-study-definition-0.1.0"
STRATEGY_DECISION_REF = (
    "data-factory/docs/reviews/"
    "vietnamese-com-trang-bat-study-strategy-decision-0.1.0.json"
)
STUDY_DEFINITION_REF = (
    "data-factory/docs/reviews/"
    "vietnamese-com-trang-bat-study-definition-0.1.0.json"
)
PREVIOUS_PLANNING_PACKET_SHA256 = "5e645369733cff3423b803b239fca0bb9bbf91697d1afe065cb2a0461f25b351"
PREVIOUS_MANIFEST_SHA256 = "d9ca020236da68486645daaa0284dfce4d49b52b63b4a4f918b86810ef6a409e"
OPTION_A_DECISION_REFERENCE = "github:issue/53#issuecomment-5884079108"
OPTION_A_STRATEGY = "narrow_reproducible_pilot"
OPTION_A_DECISION_SHA256 = "d6139ccd165db0682162dea6f43f6d0eccf021f4ea5a6a99ed5d5110f3202d43"
TARGET = "cơm trắng"
MEASURE = "bát"
PREPARATION_DEFINITION = (
    "Generic cooked white rice with no added fat, only when the user has not supplied a cultivar, "
    "variety, or materially different preparation. Cultivar distinction is not required for this "
    "generic evaluation identity."
)

STUDY_DESIGN_INPUTS = (
    "vessel_definition",
    "vessel_context_sampling_rule",
    "serving_fill_protocol",
    "minimum_independent_sample_count",
    "minimum_independent_cooking_batch_count",
    "instrument_requirements",
    "calibration_check_requirements",
    "tare_method",
    "operator_requirements",
    "measurement_date_session_rules",
    "measurement_reviewer",
    "deviation_exclusion_policy",
)

_MACHINE_REVIEWER = re.compile(r"(?:^|[^a-z0-9])(ai|agent|llm|model|bot)(?:$|[^a-z0-9])", re.IGNORECASE)


def option_a_decision_document() -> dict[str, Any]:
    return {
        "schema_version": STRATEGY_DECISION_SCHEMA_VERSION,
        "issue_reference": "github:issue/53",
        "target": {
            "normalized_vietnamese_target": TARGET,
            "canonical_measure": MEASURE,
        },
        "decision": "approved",
        "strategy": OPTION_A_STRATEGY,
        "reviewer": "pumni",
        "decision_reference": OPTION_A_DECISION_REFERENCE,
        "generalizable_to_all_vietnamese_bat": False,
        "decision_scope": "study_strategy_only",
        "food_evidence": {
            "target": TARGET,
            "reviewed_mapping_ref": REVIEWED_MAPPING_REF,
            "reviewed_mapping_schema_version": "reviewed-food-mapping-0.1.0",
            "reviewed_mapping_sha256": REVIEWED_MAPPING_SHA256,
            "decision_reference": DECISION_REFERENCE,
            "source": {
                "source_code": SOURCE_RECORD["source_code"],
                "release": SOURCE_RECORD["release"],
                "fndds_food_code": SOURCE_RECORD["fndds_food_code"],
                "fdc_id": SOURCE_RECORD["fdc_id"],
                "source_record_sha256": SOURCE_RECORD["record_sha256"],
            },
            "specificity_policy_ref": "resolve-exact-specificity-0.2.0",
            "backend_food_id": None,
        },
        "physical_study_definition_approved": False,
        "measurement_plan_values_approved": False,
        "gram_value_approved": False,
        "broader_household_study_requires_separate_decision": True,
    }


def option_a_decision_bytes() -> bytes:
    return canonical_json_bytes(option_a_decision_document())


def validate_option_a_decision(value: Any) -> tuple[dict[str, Any] | None, tuple[dict[str, str], ...]]:
    expected = option_a_decision_document()
    if not _same_typed_json(value, expected):
        return None, ({"reason_code": "strategy_decision_mismatch"},)
    return value, ()


def build_study_definition(
    decision_bytes: bytes,
    mapping_bytes: bytes,
    previous_manifest_bytes: bytes,
    previous_planning_packet_bytes: bytes,
) -> tuple[dict[str, Any] | None, tuple[dict[str, str], ...]]:
    errors: list[dict[str, str]] = []
    try:
        decision_value = json.loads(decision_bytes)
        previous_manifest_value = json.loads(previous_manifest_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None, ({"reason_code": "input_artifact_not_json"},)

    decision, decision_errors = validate_option_a_decision(decision_value)
    errors.extend(decision_errors)
    if (
        decision_bytes != option_a_decision_bytes()
        or _sha256(decision_bytes) != OPTION_A_DECISION_SHA256
    ):
        errors.append({"reason_code": "strategy_decision_bytes_mismatch"})

    if _sha256(mapping_bytes) != REVIEWED_MAPPING_SHA256:
        errors.append({"reason_code": "reviewed_mapping_hash_mismatch"})
    if _sha256(previous_manifest_bytes) != PREVIOUS_MANIFEST_SHA256:
        errors.append({"reason_code": "previous_manifest_hash_mismatch"})
    if _sha256(previous_planning_packet_bytes) != PREVIOUS_PLANNING_PACKET_SHA256:
        errors.append({"reason_code": "previous_planning_packet_hash_mismatch"})

    validated_manifest, manifest_errors = validate_portion_planning_manifest(
        previous_manifest_value, mapping_bytes
    )
    if validated_manifest is None:
        errors.extend(manifest_errors)

    previous_packet, packet_errors = build_portion_planning_packet(
        previous_manifest_bytes, mapping_bytes
    )
    if previous_packet is None:
        errors.extend(packet_errors)
    elif canonical_json_bytes(previous_packet) != previous_planning_packet_bytes:
        errors.append({"reason_code": "previous_planning_packet_content_mismatch"})

    if errors or decision is None or validated_manifest is None:
        return None, tuple(errors)

    manifest_target = validated_manifest["target"]
    evidence_identity = manifest_target["identity"]["reviewed_evidence_identity"]
    measure = manifest_target["measure"]
    definition: dict[str, Any] = {
        "schema_version": STUDY_DEFINITION_SCHEMA_VERSION,
        "policy_version": STUDY_DEFINITION_POLICY_VERSION,
        "study_id": "vietnamese-com-trang-bat-study-definition-0.1.0",
        "status": "study_design_incomplete",
        "predecessor": {
            "manifest": {
                "artifact_ref": PLANNING_MANIFEST_REF,
                "schema_version": "portion-study-manifest-0.3.0",
                "sha256": _sha256(previous_manifest_bytes),
            },
            "planning_packet": {
                "artifact_ref": "data-factory/docs/reviews/"
                "vietnamese-com-trang-bat-portion-planning-0.1.0.json",
                "schema_version": PLANNING_PACKET_SCHEMA_VERSION,
                "sha256": _sha256(previous_planning_packet_bytes),
            },
        },
        "strategy_decision": {
            "artifact_ref": STRATEGY_DECISION_REF,
            "schema_version": STRATEGY_DECISION_SCHEMA_VERSION,
            "sha256": _sha256(decision_bytes),
            "decision_reference": decision["decision_reference"],
            "decision": decision["decision"],
            "strategy": decision["strategy"],
            "reviewer": decision["reviewer"],
            "generalizable_to_all_vietnamese_bat": decision[
                "generalizable_to_all_vietnamese_bat"
            ],
            "decision_scope": decision["decision_scope"],
            "physical_study_definition_approved": decision[
                "physical_study_definition_approved"
            ],
            "measurement_plan_values_approved": decision[
                "measurement_plan_values_approved"
            ],
            "gram_value_approved": decision["gram_value_approved"],
            "broader_household_study_requires_separate_decision": decision[
                "broader_household_study_requires_separate_decision"
            ],
        },
        "target": {
            "normalized_vietnamese_target": TARGET,
            "reviewed_evidence_identity": evidence_identity,
            "backend_food_id": None,
            "preparation": manifest_target["preparation"]["definition"],
            "measure": {
                "canonical_measure": measure["canonical_measure"],
                "original_unit_phrase": measure["original_unit_phrase"],
                "represented_quantity": measure["represented_quantity"],
                "context_status": "unresolved",
                "physical_context": None,
            },
        },
        "measurement_protocol": {
            "protocol_version": PORTION_PROTOCOL_VERSION,
            "estimator_version": ESTIMATOR_VERSION,
            "bound_policy_version": BOUND_POLICY_VERSION,
        },
        "study_design_inputs": {
            name: {"status": "unresolved", "value": None, "reviewer": None, "review_ref": None}
            for name in STUDY_DESIGN_INPUTS
        },
        "future_observations": {
            "status": "not_collected",
            "observation_count": 0,
            "records": [],
        },
        "estimates": {"mass_estimate": None, "gram_estimate_emitted": False},
        "readiness": {
            "ready_for_measurement": False,
            "review_ready": False,
            "blocker_count": len(STUDY_DESIGN_INPUTS),
        },
        "publication": {
            "status": "blocked",
            "publishable": False,
            "production_eligible": False,
            "release_created": False,
            "activation_attempted": False,
        },
    }
    definition["readiness"]["ready_for_measurement"] = (
        is_study_definition_ready_for_measurement(definition)
    )
    return definition, ()


def is_study_definition_ready_for_measurement(value: Any) -> bool:
    if not isinstance(value, dict):
        return False
    strategy = value.get("strategy_decision")
    expected_strategy = {
        "artifact_ref": STRATEGY_DECISION_REF,
        "schema_version": STRATEGY_DECISION_SCHEMA_VERSION,
        "sha256": OPTION_A_DECISION_SHA256,
        "decision_reference": OPTION_A_DECISION_REFERENCE,
        "decision": "approved",
        "strategy": OPTION_A_STRATEGY,
        "reviewer": "pumni",
        "generalizable_to_all_vietnamese_bat": False,
        "decision_scope": "study_strategy_only",
        "physical_study_definition_approved": False,
        "measurement_plan_values_approved": False,
        "gram_value_approved": False,
        "broader_household_study_requires_separate_decision": True,
    }
    if not _same_typed_json(strategy, expected_strategy):
        return False
    design = value.get("study_design_inputs")
    if not isinstance(design, dict) or set(design) != set(STUDY_DESIGN_INPUTS):
        return False
    for item in design.values():
        if not isinstance(item, dict):
            return False
        reviewer = item.get("reviewer")
        if (
            item.get("status") != "reviewed"
            or item.get("value") is None
            or not _nonempty_text(reviewer)
            or _MACHINE_REVIEWER.search(reviewer)
            or not _nonempty_text(item.get("review_ref"))
        ):
            return False
    target = value.get("target")
    measure = target.get("measure") if isinstance(target, dict) else None
    expected_identity = {
        "artifact_ref": REVIEWED_MAPPING_REF,
        "schema_version": "reviewed-food-mapping-0.1.0",
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
        "specificity_policy_ref": "resolve-exact-specificity-0.2.0",
    }
    if (
        not isinstance(target, dict)
        or target.get("normalized_vietnamese_target") != TARGET
        or "backend_food_id" not in target
        or target.get("backend_food_id") is not None
        or target.get("preparation") != PREPARATION_DEFINITION
        or not _same_typed_json(target.get("reviewed_evidence_identity"), expected_identity)
        or not isinstance(measure, dict)
        or measure.get("context_status") != "reviewed"
        or measure.get("canonical_measure") != MEASURE
        or measure.get("original_unit_phrase") != MEASURE
        or type(measure.get("represented_quantity")) is not int
        or measure.get("represented_quantity") != 1
        or not _nonempty_text(measure.get("physical_context"))
    ):
        return False
    return True


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


def _nonempty_text(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def _sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()
