from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

from ..adapters.fndds_survey import FnddsParseResult, FnddsSourceRecord
from ..nutrients_fndds import (
    FNDDS_NUTRIENT_MAPPING_VERSION,
    source_nutrient_definitions,
    summarize_fndds_required_nutrients,
    summarize_fndds_source_nutrient_coverage,
)


FNDDS_REVIEW_POLICY_VERSION = "fndds-secondary-review-policy-0.1.0"
FOUNDATION_PRECEDENCE_VERSION = "foundation-primary-fndds-secondary-0.1.0"
FNDDS_ALLOWED_TARGET_IDS = frozenset(
    {
        "vmb-public-0002",
        "vmb-public-0005",
        "vmb-public-0003",
        "vmb-public-0007",
        "vmb-public-0012",
        "vmb-public-0010",
        "vmb-public-0014",
    }
)


class FnddsReviewError(ValueError):
    pass


def load_fndds_review_policy(path: Path) -> dict[str, Any]:
    try:
        raw_bytes = path.read_bytes()
        policy = json.loads(raw_bytes)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise FnddsReviewError("FNDDS review policy is not readable JSON") from error
    if not isinstance(policy, dict) or policy.get("schema_version") != FNDDS_REVIEW_POLICY_VERSION:
        raise FnddsReviewError(f"policy must use {FNDDS_REVIEW_POLICY_VERSION}")
    if policy.get("nutrient_mapping_version") != FNDDS_NUTRIENT_MAPPING_VERSION:
        raise FnddsReviewError("policy nutrient mapping version does not match the FNDDS crosswalk")
    source = policy.get("source")
    if not isinstance(source, dict) or source.get("source_code") != "usda_fndds":
        raise FnddsReviewError("policy must identify the bounded usda_fndds source")
    if source.get("rights_state") != "reference_only":
        raise FnddsReviewError("FNDDS secondary review must remain reference_only")
    precedence = policy.get("precedence")
    if (
        not isinstance(precedence, dict)
        or precedence.get("policy_version") != FOUNDATION_PRECEDENCE_VERSION
        or precedence.get("primary_source") != "usda_fdc_foundation"
        or precedence.get("secondary_source") != "usda_fndds"
        or not isinstance(precedence.get("foundation_accepted_target_ids"), list)
    ):
        raise FnddsReviewError("policy must declare Foundation-primary source precedence")
    targets = policy.get("targets")
    if not isinstance(targets, list) or not targets:
        raise FnddsReviewError("policy must contain a bounded target array")
    target_ids = [target.get("target_id") for target in targets if isinstance(target, dict)]
    if len(target_ids) != len(targets) or len(target_ids) != len(set(target_ids)):
        raise FnddsReviewError("target records must be objects with unique target IDs")
    if set(target_ids) != FNDDS_ALLOWED_TARGET_IDS:
        raise FnddsReviewError("policy target set is outside the authorized seven-food FNDDS scope")
    foundation_targets = set(precedence["foundation_accepted_target_ids"])
    if not foundation_targets.issubset(FNDDS_ALLOWED_TARGET_IDS):
        raise FnddsReviewError("Foundation precedence state contains an out-of-scope target")
    return {"policy": policy, "sha256": _sha256(raw_bytes)}


def build_fndds_review_report(
    parsed: FnddsParseResult,
    loaded_policy: dict[str, Any],
    source_evidence: dict[str, Any],
) -> dict[str, Any]:
    if not parsed.valid or len(parsed.accepted_records) != parsed.raw_record_count:
        raise FnddsReviewError("refusing candidate analysis because FNDDS parsing rejected source rows")
    if source_evidence.get("member_sha256") != parsed.source_sha256:
        raise FnddsReviewError("source evidence member hash does not match the parsed FNDDS payload")
    if source_evidence.get("schema_fingerprint") != parsed.schema_fingerprint:
        raise FnddsReviewError("source evidence schema fingerprint does not match the parsed payload")

    policy = loaded_policy.get("policy")
    policy_sha256 = loaded_policy.get("sha256")
    if not isinstance(policy, dict) or not isinstance(policy_sha256, str):
        raise FnddsReviewError("review policy must be loaded with load_fndds_review_policy")
    precedence = policy["precedence"]
    accepted_foundation = set(precedence["foundation_accepted_target_ids"])
    if not accepted_foundation.issubset(FNDDS_ALLOWED_TARGET_IDS):
        raise FnddsReviewError("Foundation precedence policy contains an out-of-scope target")

    records_by_food_code = {record.food_code: record for record in parsed.accepted_records}
    packets: list[dict[str, Any]] = []
    target_results: list[dict[str, Any]] = []
    for target in policy["targets"]:
        target_id = target["target_id"]
        base_result = {
            "target_id": target_id,
            "normalized_vietnamese_target": target["normalized_vietnamese_target"],
            "target_preparation_state": target["target_preparation_state"],
        }
        if target_id in accepted_foundation:
            target_results.append(
                {
                    **base_result,
                    "outcome": "suppressed_foundation_primary",
                    "reason_code": "accepted_foundation_mapping_exists",
                    "foundation_mapping_state": "accepted_mapping_recorded",
                }
            )
            continue

        candidate_spec = target.get("candidate")
        if candidate_spec is None:
            target_results.append(
                {
                    **base_result,
                    "outcome": "no_proposal",
                    "reason_code": target["no_proposal_reason_code"],
                    "foundation_mapping_state": "no_accepted_mapping_recorded",
                    "rejected_source_records": [
                        _source_record_reference(
                            _require_exact_record(records_by_food_code, spec, target_id),
                            reason_code=spec["reason_code"],
                        )
                        for spec in target.get("rejected_source_records", [])
                    ],
                    "literal_absence_checks": [
                        _literal_absence_result(parsed.accepted_records, check)
                        for check in target.get("literal_absence_checks", [])
                    ],
                }
            )
            continue

        record = _require_exact_record(records_by_food_code, candidate_spec, target_id)
        if record.wweia_category_description != candidate_spec.get("wweia_category_description"):
            raise FnddsReviewError(
                f"candidate record for {target_id} has an unexpected WWEIA category"
            )
        completeness = summarize_fndds_required_nutrients(record.nutrients)
        if not completeness["complete"]:
            target_results.append(
                {
                    **base_result,
                    "outcome": "no_proposal",
                    "reason_code": "required_nutrients_incomplete",
                    "foundation_mapping_state": "no_accepted_mapping_recorded",
                    "nutrient_rejections": completeness["rejected"],
                }
            )
            continue

        packet = {
            "packet_version": "fndds-source-review-packet-0.1.0",
            "proposal_id": _proposal_id(target_id, record),
            "normalized_vietnamese_target": target["normalized_vietnamese_target"],
            "target_id": target_id,
            "target_preparation_state": target["target_preparation_state"],
            "source_preparation_state": candidate_spec["source_preparation_state"],
            "semantic_rationale": candidate_spec["semantic_rationale"],
            "source_precedence": {
                "policy_version": precedence["policy_version"],
                "primary_source": precedence["primary_source"],
                "secondary_source": precedence["secondary_source"],
                "foundation_accepted_mapping_exists": False,
                "foundation_state_reference": precedence["foundation_state_reference"],
                "fallback_status": "review_proposal_pending_human_review",
            },
            "source_artifact": _source_artifact_evidence(policy, source_evidence),
            "source_record": _source_record_reference(record),
            "required_nutrient_completeness": completeness,
            "nutrient_mapping_version": FNDDS_NUTRIENT_MAPPING_VERSION,
            "source_nutrient_definitions": source_nutrient_definitions(record.nutrients),
            "rejected_alternatives": [
                _source_record_reference(
                    _require_exact_record(records_by_food_code, spec, target_id),
                    reason_code=spec["reason_code"],
                )
                for spec in target.get("rejected_alternatives", [])
            ],
            "reviewer_state": "pending_human_review",
            "reviewer_reference": None,
            "reviewer_approved": False,
            "production_eligible": False,
        }
        packets.append(packet)
        target_results.append(
            {
                **base_result,
                "outcome": "review_packet_created",
                "reason_code": None,
                "foundation_mapping_state": "no_accepted_mapping_recorded",
                "proposal_id": packet["proposal_id"],
                "source_record": packet["source_record"],
            }
        )

    coverage = summarize_fndds_source_nutrient_coverage(parsed.accepted_records)
    validation_passed = (
        len(parsed.accepted_records) == policy["source"].get("expected_record_count")
        and coverage["passed"]
    )
    return {
        "schema_version": "fndds-secondary-source-review-report-0.1.0",
        "review_policy_version": policy["schema_version"],
        "review_policy_sha256": policy_sha256,
        "project_decision_reference": policy["project_decision_reference"],
        "issue_reference": policy["issue_reference"],
        "source_artifact": _source_artifact_evidence(policy, source_evidence),
        "foundation_precedence": {
            "policy_version": precedence["policy_version"],
            "primary_source": precedence["primary_source"],
            "secondary_source": precedence["secondary_source"],
            "accepted_foundation_target_ids": sorted(accepted_foundation),
            "foundation_state_reference": precedence["foundation_state_reference"],
        },
        "importer_version": parsed.adapter_version,
        "nutrient_mapping_version": FNDDS_NUTRIENT_MAPPING_VERSION,
        "validation_report": {
            "passed": validation_passed,
            "source_parse": parsed.to_dict(),
            "nutrient_coverage": coverage,
        },
        "counts": {
            "raw_record_count": parsed.raw_record_count,
            "accepted_record_count": len(parsed.accepted_records),
            "rejected_record_count": len(parsed.rejected_records),
            "target_count": len(target_results),
            "candidate_packet_count": len(packets),
            "no_proposal_count": sum(item["outcome"] == "no_proposal" for item in target_results),
            "foundation_suppressed_count": sum(
                item["outcome"] == "suppressed_foundation_primary" for item in target_results
            ),
        },
        "candidate_packets": packets,
        "target_results": target_results,
        "production_eligible": False,
        "staged_candidate_created": False,
        "activation_attempted": False,
    }


def _source_artifact_evidence(
    policy: dict[str, Any], source_evidence: dict[str, Any]
) -> dict[str, Any]:
    source = policy["source"]
    return {
        "source_code": source["source_code"],
        "publisher": source["publisher"],
        "release": source["release"],
        "release_date": source["release_date"],
        "archive_filename": source_evidence["archive_filename"],
        "archive_locator": source["archive_locator"],
        "archive_sha256": source_evidence.get("archive_sha256"),
        "archive_size_bytes": source_evidence.get("archive_size_bytes"),
        "member_name": source_evidence["member_name"],
        "member_sha256": source_evidence["member_sha256"],
        "member_size_bytes": source_evidence.get("member_size_bytes"),
        "schema_fingerprint": source_evidence["schema_fingerprint"],
        "rights_state": source["rights_state"],
        "rights_license": source["rights_license"],
        "rights_evidence": source["rights_evidence"],
        "rights_citation": source["rights_citation"],
        "project_decision_reference": policy["project_decision_reference"],
    }


def _require_exact_record(
    records_by_food_code: dict[str, FnddsSourceRecord],
    spec: dict[str, Any],
    target_id: str,
) -> FnddsSourceRecord:
    record = records_by_food_code.get(spec["fndds_food_code"])
    if record is None:
        raise FnddsReviewError(f"source record for {target_id} is absent from pinned FNDDS data")
    if record.fdc_id != spec["fdc_id"] or record.description != spec["description"]:
        raise FnddsReviewError(f"source ID or exact description changed for {target_id}")
    return record


def _source_record_reference(
    record: FnddsSourceRecord, *, reason_code: str | None = None
) -> dict[str, Any]:
    result = {
        "source_code": "usda_fndds",
        "fndds_food_code": record.food_code,
        "fdc_id": record.fdc_id,
        "description": record.description,
        "wweia_category_description": record.wweia_category_description,
        "record_sha256": record.payload_sha256,
    }
    if reason_code is not None:
        result["reason_code"] = reason_code
    return result


def _literal_absence_result(
    records: tuple[FnddsSourceRecord, ...], check: dict[str, Any]
) -> dict[str, Any]:
    terms = check.get("all_terms")
    if not isinstance(terms, list) or not terms or any(not isinstance(term, str) for term in terms):
        raise FnddsReviewError("literal absence check must contain non-empty string terms")
    folded_terms = [term.casefold() for term in terms]
    matches = [
        record
        for record in records
        if all(term in record.description.casefold() for term in folded_terms)
    ]
    result = {
        "all_terms": terms,
        "match_count": len(matches),
        "matched_source_records": [_source_record_reference(record) for record in matches],
    }
    if "description" in check:
        result["description"] = check["description"]
    return result


def _proposal_id(target_id: str, record: FnddsSourceRecord) -> str:
    identity = f"{FNDDS_REVIEW_POLICY_VERSION}:{target_id}:{record.food_code}:{record.payload_sha256}"
    suffix = hashlib.sha256(identity.encode("utf-8")).hexdigest()[:20]
    return f"fndds-proposal-{suffix}"


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()
