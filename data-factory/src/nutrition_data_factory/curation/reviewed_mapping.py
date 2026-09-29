from __future__ import annotations

import hashlib
import json
from typing import Any

from .packets import (
    CURATION_DECISION_POLICY_VERSION,
    CurationDecision,
    DecisionHistory,
    SourceRecordIdentity,
    decision_id,
)


REVIEWED_MAPPING_VERSION = "reviewed-food-mapping-0.1.0"
DECISION_REFERENCE = "github:issue/35#issuecomment-5882043136"
SOURCE_STRATEGY_REFERENCE = "github:issue/41#issuecomment-5867171847"
REVIEWER = "pumni"
REVIEWED_AT = "2026-09-29T01:43:47Z"
PROPOSAL_ID = "fndds-proposal-d7772be2123024935757"
REPORT_SHA256 = "8cb33094aa51a6b02659c314298d3e106530a7d0306ab0c162f5e865fee5dc50"
SOURCE_REGISTRY_SHA256 = "9a05078804e98f9ff7e17d7a6815a035dc140a759c5fb6f385037547b50716da"
SOURCE_RECORD = {
    "source_code": "usda_fndds",
    "release": "2021-2023 / October 2024",
    "fndds_food_code": "56205008",
    "fdc_id": 2708408,
    "description": "Rice, white, cooked, no added fat",
    "record_sha256": "634cf6fabbcc9ccb76f513ef274a9dcc3d92f495c78792cf8221170d98a102cc",
    "archive_sha256": "dfb06ae7ddc397ccd570b91c14b75438ab2ba39f64f22d321f61d4a52a77f3eb",
    "archive_member": "surveyDownload.json",
    "member_sha256": "2e7eb9fda92adf1d4d784dba5eaa3a7fd4418cd86ccff383c7c9294d79e9b808",
    "schema_fingerprint": "bc072c4e5cf81ebb632ddb099d502e4e8a5cfef55933b9a18a60840a86e6e4f4",
    "nutrient_mapping_version": "fndds-required-nutrients-0.1.0",
}
UNSUPPORTED_FNDDS_TARGETS = {
    "vmb-public-0005": "trứng gà luộc",
    "vmb-public-0003": "thịt bò",
    "vmb-public-0007": "thịt gà",
    "vmb-public-0012": "thịt gà luộc",
    "vmb-public-0010": "sữa tươi",
    "vmb-public-0014": "rau muống",
}
COMPOSITE_RECIPE_TARGETS = ("phở bò", "bún bò Huế", "cơm gà")


class ReviewedMappingError(ValueError):
    pass


def materialize_reviewed_rice_mapping(
    report_bytes: bytes, source_registry_bytes: bytes
) -> tuple[bytes, bytes]:
    """Validate pinned historical evidence and return deterministic decision and mapping bytes."""
    try:
        report = json.loads(report_bytes)
        registry = json.loads(source_registry_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ReviewedMappingError("review evidence must be valid UTF-8 JSON") from error
    if not isinstance(report, dict) or not isinstance(registry, dict):
        raise ReviewedMappingError("review evidence must contain JSON objects")

    packet = _validate_evidence(report, registry)
    if _sha256(report_bytes) != REPORT_SHA256:
        raise ReviewedMappingError("historical #44 proposal artifact SHA-256 changed")

    source_identity = SourceRecordIdentity(
        source_code=SOURCE_RECORD["source_code"],
        release=SOURCE_RECORD["release"],
        source_record_id=SOURCE_RECORD["fndds_food_code"],
        external_id_type="fdc_id",
        external_record_id=str(SOURCE_RECORD["fdc_id"]),
        description=SOURCE_RECORD["description"],
        record_sha256=SOURCE_RECORD["record_sha256"],
        archive_sha256=SOURCE_RECORD["archive_sha256"],
        archive_member=SOURCE_RECORD["archive_member"],
        member_sha256=SOURCE_RECORD["member_sha256"],
        schema_fingerprint=SOURCE_RECORD["schema_fingerprint"],
    )
    rationale = (
        "Approved for generic cooked white rice with no added fat when the user has not supplied a "
        "cultivar, variety, or materially different preparation. Cultivar distinction is not required "
        "for this generic evaluation identity. More-specific identities are not approved to collapse "
        "into generic cơm trắng."
    )
    decision = CurationDecision(
        decision_id=decision_id(PROPOSAL_ID, REVIEWED_AT, REVIEWER),
        decision_type="food_source_mapping",
        candidate_id=PROPOSAL_ID,
        decision="approved",
        reviewer=REVIEWER,
        reviewed_at=REVIEWED_AT,
        rationale=rationale,
        evidence_refs=(
            DECISION_REFERENCE,
            SOURCE_STRATEGY_REFERENCE,
            f"data-factory/docs/reviews/fndds-secondary-source-issue-44.json#sha256={REPORT_SHA256}",
            "data-factory/docs/reviews/fndds-secondary-source-issue-44.json"
            f"#source_registry_evidence.sha256={SOURCE_REGISTRY_SHA256}",
            f"usda_fndds:{SOURCE_RECORD['fndds_food_code']}#record-sha256={SOURCE_RECORD['record_sha256']}",
        ),
        policy_version=CURATION_DECISION_POLICY_VERSION,
        decision_reference=DECISION_REFERENCE,
        source_record_identity=source_identity,
    )
    history = DecisionHistory()
    history.append(decision)
    decision_bytes = history.to_jsonl().encode("utf-8")
    mapping = _build_mapping_artifact(decision, rationale, packet, report, registry)
    mapping_bytes = _canonical_json_bytes(mapping)
    return decision_bytes, mapping_bytes


def _validate_evidence(report: dict[str, Any], registry: dict[str, Any]) -> dict[str, Any]:
    # The historical #44 report pins the registry snapshot. The current registry is live
    # configuration, so only the FNDDS safety fields are checked below.
    if report.get("schema_version") != "fndds-secondary-source-review-report-0.1.0":
        raise ReviewedMappingError("unexpected historical FNDDS report version")
    counts = report.get("counts")
    if counts != {
        "raw_record_count": 5432,
        "accepted_record_count": 5432,
        "rejected_record_count": 0,
        "target_count": 7,
        "candidate_packet_count": 1,
        "no_proposal_count": 6,
        "foundation_suppressed_count": 0,
    }:
        raise ReviewedMappingError("historical FNDDS report target counts changed")
    if report.get("production_eligible") is not False:
        raise ReviewedMappingError("historical FNDDS report must remain production-ineligible")
    if report.get("staged_candidate_created") is not False or report.get("activation_attempted") is not False:
        raise ReviewedMappingError("historical FNDDS report must remain unactivated and unstaged")
    if report.get("project_decision_reference") != SOURCE_STRATEGY_REFERENCE:
        raise ReviewedMappingError("FNDDS report source-strategy decision reference changed")

    source_artifact = report.get("source_artifact")
    expected_source_artifact = {
        "source_code": "usda_fndds",
        "release": "2021-2023",
        "release_date": "2024-10",
        "archive_filename": "FoodData_Central_survey_food_json_2024-10-31.zip",
        "archive_sha256": SOURCE_RECORD["archive_sha256"],
        "member_name": SOURCE_RECORD["archive_member"],
        "member_sha256": SOURCE_RECORD["member_sha256"],
        "schema_fingerprint": SOURCE_RECORD["schema_fingerprint"],
        "rights_state": "reference_only",
        "project_decision_reference": SOURCE_STRATEGY_REFERENCE,
    }
    if not isinstance(source_artifact, dict) or any(
        source_artifact.get(key) != value for key, value in expected_source_artifact.items()
    ):
        raise ReviewedMappingError("pinned FNDDS archive/member/schema evidence changed")
    if report.get("nutrient_mapping_version") != SOURCE_RECORD["nutrient_mapping_version"]:
        raise ReviewedMappingError("FNDDS nutrient mapping version changed")

    packets = report.get("candidate_packets")
    if not isinstance(packets, list) or len(packets) != 1:
        raise ReviewedMappingError("expected exactly one historical FNDDS proposal")
    packet = packets[0]
    if packet.get("proposal_id") != PROPOSAL_ID:
        raise ReviewedMappingError("FNDDS proposal ID changed")
    if packet.get("target_id") != "vmb-public-0002" or packet.get("normalized_vietnamese_target") != "cơm trắng":
        raise ReviewedMappingError("FNDDS proposal target changed")
    if (
        packet.get("reviewer_state") != "pending_human_review"
        or packet.get("reviewer_reference") is not None
        or packet.get("reviewer_approved") is not False
    ):
        raise ReviewedMappingError("historical FNDDS proposal must remain pending human review")
    if packet.get("production_eligible") is not False:
        raise ReviewedMappingError("historical proposal must remain production-ineligible")
    if packet.get("nutrient_mapping_version") != SOURCE_RECORD["nutrient_mapping_version"]:
        raise ReviewedMappingError("proposal nutrient mapping version changed")
    completeness = packet.get("required_nutrient_completeness")
    if not isinstance(completeness, dict) or completeness.get("complete") is not True:
        raise ReviewedMappingError("proposal must retain complete mapped nutrient evidence")

    record = packet.get("source_record")
    expected_record = {
        "source_code": SOURCE_RECORD["source_code"],
        "fndds_food_code": SOURCE_RECORD["fndds_food_code"],
        "fdc_id": SOURCE_RECORD["fdc_id"],
        "description": SOURCE_RECORD["description"],
        "record_sha256": SOURCE_RECORD["record_sha256"],
    }
    if not isinstance(record, dict) or any(record.get(key) != value for key, value in expected_record.items()):
        raise ReviewedMappingError("exact FNDDS/FDC source record identity or SHA-256 changed")

    target_results = report.get("target_results")
    if not isinstance(target_results, list) or len(target_results) != 7:
        raise ReviewedMappingError("historical FNDDS bounded target set changed")
    by_id = {item.get("target_id"): item for item in target_results if isinstance(item, dict)}
    expected_targets = {"vmb-public-0002": "cơm trắng", **UNSUPPORTED_FNDDS_TARGETS}
    if len(by_id) != 7 or {key: item.get("normalized_vietnamese_target") for key, item in by_id.items()} != expected_targets:
        raise ReviewedMappingError("historical FNDDS target identities changed")
    for target_id in UNSUPPORTED_FNDDS_TARGETS:
        result = by_id[target_id]
        if result.get("outcome") != "no_proposal":
            raise ReviewedMappingError("one of the other six FNDDS targets is no longer unsupported")
    if by_id["vmb-public-0002"].get("outcome") != "review_packet_created":
        raise ReviewedMappingError("historical generic rice proposal outcome changed")

    registry_source = _find_source(registry, "usda_fndds")
    if (
        registry_source.get("rights_state") != "reference_only"
        or registry_source.get("production_ingestion") != "blocked_until_product_selection"
        or registry_source.get("allowed_uses") != ["analysis", "reference"]
        or "staged_candidate" not in registry_source.get("prohibited_uses", [])
        or registry_source.get("production_eligible") is not False
    ):
        raise ReviewedMappingError("FNDDS source registry must remain source-wide non-staging")
    registry_evidence = report.get("source_registry_evidence")
    if (
        not isinstance(registry_evidence, dict)
        or registry_evidence.get("sha256") != SOURCE_REGISTRY_SHA256
        or registry_evidence.get("source_code") != "usda_fndds"
        or registry_evidence.get("rights_state") != "reference_only"
        or registry_evidence.get("production_ingestion") != "blocked_until_product_selection"
        or registry_evidence.get("allowed_uses") != ["analysis", "reference"]
        or "staged_candidate" not in registry_evidence.get("prohibited_uses", [])
        or registry_evidence.get("production_eligible") is not False
    ):
        raise ReviewedMappingError("historical source registry evidence changed")
    return packet


def _find_source(registry: dict[str, Any], code: str) -> dict[str, Any]:
    sources = registry.get("sources")
    if not isinstance(sources, list):
        raise ReviewedMappingError("source registry has no source list")
    matches = [source for source in sources if isinstance(source, dict) and source.get("code") == code]
    if len(matches) != 1:
        raise ReviewedMappingError("source registry must contain exactly one FNDDS source")
    return matches[0]


def _build_mapping_artifact(
    decision: CurationDecision,
    rationale: str,
    packet: dict[str, Any],
    report: dict[str, Any],
    registry: dict[str, Any],
) -> dict[str, Any]:
    unresolved = [
        {
            "target_id": target_id,
            "normalized_vietnamese_target": target,
            "identity_status": "unsupported",
            "composition_status": "unsupported",
            "fndds_outcome": "no_proposal",
        }
        for target_id, target in UNSUPPORTED_FNDDS_TARGETS.items()
    ]
    registry_source = _find_source(registry, "usda_fndds")
    return {
        "schema_version": REVIEWED_MAPPING_VERSION,
        "issue_reference": "github:issue/35",
        "review_decision": {
            "decision_id": decision.decision_id,
            "decision_type": decision.decision_type,
            "candidate_id": decision.candidate_id,
            "decision": decision.decision,
            "decision_reference": decision.decision_reference,
            "reviewer": decision.reviewer,
            "reviewed_at": decision.reviewed_at,
            "rationale": rationale,
            "curation_policy_version": decision.policy_version,
            "evidence_refs": list(decision.evidence_refs),
        },
        "mapping": {
            "normalized_vietnamese_target": "cơm trắng",
            "identity_status": "reviewed_supported",
            "composition_status": "reviewed_supported",
            "semantic_boundary": (
                "Generic cooked white rice with no added fat, only when the user has not supplied a "
                "cultivar, variety, or materially different preparation. Cultivar distinction is not "
                "required for this generic evaluation identity."
            ),
            "source_record": {
                **SOURCE_RECORD,
                "proposal_id": packet["proposal_id"],
                "source_strategy_decision_reference": report["project_decision_reference"],
            },
            "runtime_specificity_policy_reference": "resolve-exact-specificity-0.2.0",
            "not_approved_specificity": [
                "jasmine rice or jasmine cultivar",
                "ST25 or another named cultivar",
                "nếp / glutinous rice",
                "gạo lứt / brown rice",
                "rang / chiên / fried rice",
                "rice prepared with added oil or other added fat",
                "another materially different preparation",
            ],
            "portion_boundary": {
                "status": "unsupported_without_explicit_grams_or_later_reviewed_portion_evidence",
                "unit_portion_inference_approved": False,
                "gram_weights_emitted": False,
            },
            "recipe_boundary": {
                "status": "separate_recipe_evidence_required_for_composite_dishes",
                "recipe_evidence_emitted": False,
            },
            "production_eligible": False,
            "activation_authorized": False,
        },
        "coverage_update": {
            "other_bounded_fndds_targets": unresolved,
            "composite_dishes": [
                {"normalized_vietnamese_target": phrase, "status": "recipe_required"}
                for phrase in COMPOSITE_RECIPE_TARGETS
            ],
            "synthetic_fixture_bat_grams_promoted": False,
        },
        "source_registry_boundary": {
            "historical_report_registry_sha256": report["source_registry_evidence"]["sha256"],
            "rights_state": registry_source["rights_state"],
            "production_ingestion": registry_source["production_ingestion"],
            "allowed_uses": registry_source["allowed_uses"],
            "source_wide_staged_candidate_permission": "staged_candidate"
            in registry_source["allowed_uses"],
            "production_eligible": registry_source["production_eligible"],
        },
    }


def canonical_json_bytes(value: Any) -> bytes:
    return _canonical_json_bytes(value)


def _canonical_json_bytes(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def sha256(payload: bytes) -> str:
    return _sha256(payload)


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()
