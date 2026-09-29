from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

from ..adapters.fndds_survey import (
    FNDDS_ADAPTER_VERSION,
    FNDDS_ARCHIVE_FILENAME,
    FNDDS_ARCHIVE_MEMBER,
    FNDDS_EXPECTED_RECORD_COUNT,
    FNDDS_RELEASE,
    FnddsSourceRecord,
    FnddsSurveyAdapter,
    extract_pinned_archive_member,
)
from ..models import SourceMetadata
from ..nutrients_fndds import FNDDS_NUTRIENT_MAPPING_VERSION, extract_fndds_required_nutrient_values
from ..source_registry import SourceRegistry
from .reviewed_mapping import DECISION_REFERENCE, SOURCE_RECORD


REVIEWED_COMPOSITION_SCHEMA_VERSION = "reviewed-food-composition-0.1.0"
REVIEWED_COMPOSITION_POLICY_VERSION = "reviewed-composition-evaluation-0.1.0"
REVIEWED_MAPPING_REF = (
    "data-factory/docs/reviews/"
    "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json"
)
REVIEWED_MAPPING_SHA256 = "c7ab626ed66c01a84c6b00d7e312a3e061eef4af2ec4dcfed950e6b398307324"
REVIEWED_COMPOSITION_REF = (
    "data-factory/docs/reviews/vietnamese-com-trang-reviewed-composition-0.1.0.json"
)
REQUIRED_NUTRIENT_TARGETS = frozenset(
    {"energy_kcal", "protein_g", "carbohydrate_g", "fat_g"}
)


class ReviewedCompositionError(ValueError):
    pass


def validate_reviewed_mapping(mapping_bytes: bytes) -> dict[str, Any]:
    if _sha256(mapping_bytes) != REVIEWED_MAPPING_SHA256:
        raise ReviewedCompositionError("reviewed mapping SHA-256 does not match the approved mapping")
    try:
        mapping = json.loads(mapping_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ReviewedCompositionError("reviewed mapping is not valid UTF-8 JSON") from error
    if not isinstance(mapping, dict) or mapping.get("schema_version") != "reviewed-food-mapping-0.1.0":
        raise ReviewedCompositionError("reviewed mapping version is not approved")

    identity = mapping.get("mapping")
    decision = mapping.get("review_decision")
    if not isinstance(identity, dict) or not isinstance(decision, dict):
        raise ReviewedCompositionError("reviewed mapping identity and decision are required")
    if (
        identity.get("normalized_vietnamese_target") != "cơm trắng"
        or identity.get("identity_status") != "reviewed_supported"
        or identity.get("composition_status") != "reviewed_supported"
        or identity.get("activation_authorized") is not False
    ):
        raise ReviewedCompositionError("reviewed mapping target or evidence state changed")
    if decision.get("decision") != "approved" or decision.get("decision_reference") != DECISION_REFERENCE:
        raise ReviewedCompositionError("reviewed mapping decision reference changed")
    source_record = identity.get("source_record")
    if not isinstance(source_record, dict) or any(
        source_record.get(key) != expected for key, expected in SOURCE_RECORD.items()
    ):
        raise ReviewedCompositionError("reviewed mapping source record identity changed")
    if mapping.get("source_registry_boundary", {}).get("source_wide_staged_candidate_permission") is not False:
        raise ReviewedCompositionError("reviewed mapping no longer records the source-wide staging boundary")
    return mapping


def validate_fndds_source_policy(policy: dict[str, Any]) -> dict[str, Any]:
    source = policy.get("source") if isinstance(policy, dict) else None
    if not isinstance(source, dict):
        raise ReviewedCompositionError("FNDDS source policy is missing")
    expected = {
        "source_code": "usda_fndds",
        "release": FNDDS_RELEASE,
        "archive_filename": FNDDS_ARCHIVE_FILENAME,
        "archive_sha256": SOURCE_RECORD["archive_sha256"],
        "archive_member": FNDDS_ARCHIVE_MEMBER,
        "member_sha256": SOURCE_RECORD["member_sha256"],
        "schema_fingerprint": SOURCE_RECORD["schema_fingerprint"],
        "expected_record_count": FNDDS_EXPECTED_RECORD_COUNT,
        "rights_state": "reference_only",
    }
    if any(source.get(key) != value for key, value in expected.items()):
        raise ReviewedCompositionError("FNDDS source policy does not match the pinned release")
    if policy.get("nutrient_mapping_version") != FNDDS_NUTRIENT_MAPPING_VERSION:
        raise ReviewedCompositionError("FNDDS nutrient mapping version changed")
    return source


def validate_fndds_source_registry(source: SourceMetadata) -> None:
    if (
        source.code != "usda_fndds"
        or source.release != SOURCE_RECORD["release"]
        or source.rights_state != "reference_only"
        or source.production_ingestion != "blocked_until_product_selection"
        or set(source.allowed_uses) != {"analysis", "reference"}
        or len(source.allowed_uses) != 2
        or "staged_candidate" not in source.prohibited_uses
        or source.production_eligible is not False
    ):
        raise ReviewedCompositionError("current FNDDS source registry safety policy changed")


def build_reviewed_rice_composition(
    record: FnddsSourceRecord,
    mapping_bytes: bytes,
    archive_evidence: dict[str, Any],
    source_registry: SourceRegistry,
    *,
    adapter_version: str = FNDDS_ADAPTER_VERSION,
) -> bytes:
    mapping = validate_reviewed_mapping(mapping_bytes)
    validate_fndds_source_registry(source_registry.get("usda_fndds"))
    source = mapping["mapping"]["source_record"]
    _validate_selected_record(record, source)
    _validate_archive_evidence(archive_evidence, source)
    if adapter_version != FNDDS_ADAPTER_VERSION:
        raise ReviewedCompositionError("FNDDS adapter version changed")
    if source.get("nutrient_mapping_version") != FNDDS_NUTRIENT_MAPPING_VERSION:
        raise ReviewedCompositionError("reviewed mapping nutrient policy changed")

    try:
        nutrients = extract_fndds_required_nutrient_values(record)
    except ValueError as error:
        raise ReviewedCompositionError(str(error)) from error
    if len(nutrients) != 4 or {item["target_code"] for item in nutrients} != REQUIRED_NUTRIENT_TARGETS:
        raise ReviewedCompositionError("exactly four required FNDDS nutrients must be materialized")

    artifact = {
        "schema_version": REVIEWED_COMPOSITION_SCHEMA_VERSION,
        "policy_version": REVIEWED_COMPOSITION_POLICY_VERSION,
        "normalized_vietnamese_target": mapping["mapping"]["normalized_vietnamese_target"],
        "semantic_boundary": mapping["mapping"]["semantic_boundary"],
        "reviewed_mapping_ref": REVIEWED_MAPPING_REF,
        "reviewed_mapping_sha256": REVIEWED_MAPPING_SHA256,
        "owner_domain_decision_ref": mapping["review_decision"]["decision_reference"],
        "specificity_policy_reference": mapping["mapping"]["runtime_specificity_policy_reference"],
        "source_code": source["source_code"],
        "source_release": source["release"],
        "fndds_food_code": source["fndds_food_code"],
        "fdc_id": source["fdc_id"],
        "source_description": source["description"],
        "source_record_sha256": source["record_sha256"],
        "archive_filename": FNDDS_ARCHIVE_FILENAME,
        "archive_sha256": archive_evidence["archive_sha256"],
        "archive_member": archive_evidence["archive_member"],
        "member_sha256": archive_evidence["member_sha256"],
        "schema_fingerprint": archive_evidence["schema_fingerprint"],
        "fndds_adapter_version": adapter_version,
        "nutrient_mapping_version": FNDDS_NUTRIENT_MAPPING_VERSION,
        "basis_amount": 100,
        "basis_unit": "g",
        "edible_basis": True,
        "nutrients": nutrients,
        "evaluation_eligible": True,
        "catalog_staging_authorized": False,
        "production_eligible": False,
        "activation_authorized": False,
        "portion_evidence_authorized": False,
        "recipe_evidence_authorized": False,
    }
    return (json.dumps(artifact, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def materialize_reviewed_rice_composition(
    archive_bytes: bytes,
    archive_filename: str,
    mapping_bytes: bytes,
    loaded_policy: dict[str, Any],
    source_registry: SourceRegistry,
) -> bytes:
    mapping = validate_reviewed_mapping(mapping_bytes)
    policy = loaded_policy.get("policy")
    if not isinstance(policy, dict):
        raise ReviewedCompositionError("FNDDS review policy was not loaded")
    source_policy = validate_fndds_source_policy(policy)
    if not isinstance(loaded_policy.get("sha256"), str) or len(loaded_policy["sha256"]) != 64:
        raise ReviewedCompositionError("FNDDS review policy hash is invalid")
    validate_fndds_source_registry(source_registry.get("usda_fndds"))

    member_bytes = extract_pinned_archive_member(archive_bytes, archive_filename, source_policy)
    parsed = FnddsSurveyAdapter().parse(
        member_bytes,
        release=FNDDS_RELEASE,
        expected_sha256=source_policy["member_sha256"],
    )
    if (
        parsed.schema_fingerprint != source_policy["schema_fingerprint"]
        or parsed.raw_record_count != FNDDS_EXPECTED_RECORD_COUNT
        or not parsed.valid
        or len(parsed.accepted_records) != FNDDS_EXPECTED_RECORD_COUNT
    ):
        raise ReviewedCompositionError("pinned FNDDS dataset failed full-release validation")

    source = mapping["mapping"]["source_record"]
    selected = [record for record in parsed.accepted_records if record.food_code == source["fndds_food_code"]]
    if len(selected) != 1:
        raise ReviewedCompositionError("exactly one reviewed FNDDS food code must be present")
    archive_evidence = {
        "archive_sha256": _sha256(archive_bytes),
        "archive_member": FNDDS_ARCHIVE_MEMBER,
        "member_sha256": parsed.source_sha256,
        "schema_fingerprint": parsed.schema_fingerprint,
    }
    return build_reviewed_rice_composition(
        selected[0],
        mapping_bytes,
        archive_evidence,
        source_registry,
        adapter_version=parsed.adapter_version,
    )


def write_immutable_artifact(output_path: Path, artifact_bytes: bytes) -> None:
    output = output_path.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        if output.read_bytes() != artifact_bytes:
            raise ReviewedCompositionError(f"refusing to overwrite a different artifact: {output}")
        return
    output.write_bytes(artifact_bytes)


def _validate_selected_record(record: FnddsSourceRecord, source: dict[str, Any]) -> None:
    expected = {
        "food_code": source["fndds_food_code"],
        "fdc_id": source["fdc_id"],
        "description": source["description"],
        "payload_sha256": source["record_sha256"],
    }
    if any(getattr(record, key) != value for key, value in expected.items()):
        raise ReviewedCompositionError("selected FNDDS record identity or source hash changed")


def _validate_archive_evidence(evidence: dict[str, Any], source: dict[str, Any]) -> None:
    expected = {
        "archive_sha256": source["archive_sha256"],
        "archive_member": source["archive_member"],
        "member_sha256": source["member_sha256"],
        "schema_fingerprint": source["schema_fingerprint"],
    }
    if any(evidence.get(key) != value for key, value in expected.items()):
        raise ReviewedCompositionError("pinned FNDDS archive/member/schema evidence changed")


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()
