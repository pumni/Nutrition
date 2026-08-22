"""Deterministic producer for the canonical catalog-handoff v1 package.

This module consumes the already parsed/normalized release objects. It deliberately does not read
the source archive again and has no database or network boundary.
"""

from __future__ import annotations

import hashlib
import json
import math
import re
from pathlib import Path
from typing import Any, Iterable

from ..adapters.fdc_foundation import FDC_FOUNDATION_RELEASE, FdcSourceRecord
from ..artifacts import ArtifactRef
from ..compatibility import load_compatibility_manifest, selection_fingerprint
from ..models import SourceMetadata


CONTRACT_VERSION = "catalog-handoff-1.0.0"
PACKAGE_KIND = "nutrition-catalog-handoff"
HANDOFF_PROFILE = "fdc-foundation-reviewed-selection-v1"
PRODUCER_VERSION = "nutrition-data-factory-catalog-handoff-0.1.0"
FDC_SOURCE_CODE = "usda_fdc_foundation"
NUTRIENT_CROSSWALK_POLICY_VERSION = "fdc-nutrient-crosswalk-0.2.0"
SUPPORTED_NUTRIENT_CODES = frozenset({"energy_kcal", "protein_g", "fat_g", "carbohydrate_g"})
_SHA256 = re.compile(r"^[0-9a-f]{64}$")
_SAFE_PATH = re.compile(r"^[A-Za-z0-9._/-]+$")


def compile_catalog_handoff_v1(
    output_dir: Path,
    *,
    metadata: SourceMetadata,
    archive_artifact: ArtifactRef,
    extracted_artifact: ArtifactRef,
    source_records: list[FdcSourceRecord],
    composition_values: list[dict[str, Any]],
    food_concepts: list[dict[str, Any]],
    food_names: list[dict[str, Any]],
    source_food_mappings: list[dict[str, Any]],
    backend_baseline: str,
    selection_path: Path | None = None,
    source_schema_fingerprint: str = "fdc-foundation-json-0.1.0",
) -> Path:
    """Compile the reviewed FDC selection into a contract-bound package.

    The selection manifest is loaded from the repository configuration and its fingerprint is
    checked before any output is written. Unsupported source observations are omitted from the
    handoff, while the full release package continues to preserve them for evidence/review.
    """

    output_dir = output_dir.resolve()
    if output_dir.exists() and any(output_dir.iterdir()):
        raise ValueError(f"catalog handoff output is not empty: {output_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)
    selection_file = selection_path or Path(__file__).resolve().parents[3] / "config" / "backend-fdc-selection.json"
    if metadata.code != FDC_SOURCE_CODE:
        raise ValueError(f"catalog handoff v1 requires source code {FDC_SOURCE_CODE}")
    if metadata.release != FDC_FOUNDATION_RELEASE:
        raise ValueError(f"catalog handoff v1 requires release {FDC_FOUNDATION_RELEASE}")
    if metadata.production_eligible:
        raise ValueError("catalog handoff v1 cannot consume a production-eligible source")
    if not metadata.locator.strip() or not metadata.rights_state.strip():
        raise ValueError("catalog handoff v1 requires source locator and rights state")
    for artifact in (archive_artifact, extracted_artifact):
        if not _SHA256.fullmatch(artifact.sha256.lower()) or artifact.size <= 0:
            raise ValueError("catalog handoff artifacts require a lowercase SHA-256 and positive size")
    selection = load_compatibility_manifest(selection_file)
    selected_ids = tuple(sorted(selection["fdc_ids"]))
    expected_selection_sha256 = selection_fingerprint(selected_ids)
    if selection.get("selection_sha256") != expected_selection_sha256:
        raise ValueError("reviewed selection fingerprint does not match its IDs")
    if selection.get("auto_add_records") is not False or selection.get("production_eligible") is not False:
        raise ValueError("reviewed selection is not safe for a staged-only handoff")
    if len(selected_ids) != 20:
        raise ValueError("catalog handoff v1 requires the reviewed 20-record selection")

    records = _canonical_records(source_records, metadata)
    _validate_unique([record["source_id"] for record in records], "source record")
    by_id = {int(record["source_id"]): record for record in records}
    missing = [str(item) for item in selected_ids if item not in by_id]
    if missing:
        raise ValueError(f"reviewed selection is missing source records: {','.join(missing)}")
    selected_records = [by_id[item] for item in selected_ids]

    concepts = _canonical_concepts(food_concepts, selected_records)
    names = _canonical_names(food_names, selected_records, concepts)
    mappings = _canonical_mappings(source_food_mappings, selected_records, concepts)
    compositions = _canonical_compositions(composition_values, selected_records)
    _validate_profile_completeness(selected_records, compositions)
    _validate_references(selected_records, concepts, names, mappings, compositions)

    dataset_release = {
        "dataset_code": metadata.code,
        "version": metadata.release,
        "status": "staged_candidate",
        "artifact_sha256": extracted_artifact.sha256.lower(),
        "archive_sha256": archive_artifact.sha256.lower(),
        "object_uri": metadata.locator,
        "schema_fingerprint": source_schema_fingerprint,
        "source_rights_state": metadata.rights_state,
        "record_count": len(selected_records),
        "production_eligible": False,
    }
    source_release = {
        "source_code": metadata.code,
        "release": metadata.release,
        "publisher": metadata.publisher,
        "purpose": metadata.purpose,
        "locator": metadata.locator,
        "rights_state": metadata.rights_state,
        "production_eligible": False,
        "archive_artifact": archive_artifact.to_dict(),
        "extracted_artifact": extracted_artifact.to_dict(),
    }

    payloads: list[tuple[str, str, str, list[dict[str, Any]] | dict[str, Any]]] = [
        ("dataset-release.json", "dataset_release", "dataset-release", dataset_release),
        ("source-releases.json", "source_releases", "source-release", {"sources": [source_release]}),
        ("raw-source-records.jsonl", "raw_source_records", "raw-source-record", selected_records),
        ("food-concepts.jsonl", "food_concepts", "food-concept", concepts),
        ("food-names.jsonl", "food_names", "food-name", names),
        ("source-food-mappings.jsonl", "source_food_mappings", "source-food-mapping", mappings),
        ("composition-values.jsonl", "composition_values", "composition-value", compositions),
    ]
    for filename, _role, _schema, value in payloads:
        if filename.endswith(".jsonl"):
            _write_jsonl(output_dir / filename, value)  # type: ignore[arg-type]
        else:
            _write_json(output_dir / filename, value)

    file_entries = []
    for filename, role, schema, value in payloads:
        path = output_dir / filename
        _assert_safe_relative_path(filename)
        file_entries.append({
            "path": filename,
            "role": role,
            "schema": schema,
            "sha256": _sha256(path.read_bytes()),
            "size_bytes": path.stat().st_size,
            "record_count": len(value) if isinstance(value, list) else 1,
        })
    package_key = f"{metadata.code}:{metadata.release}:{expected_selection_sha256}:{backend_baseline}"
    package_id = f"catalog-handoff-v1-{_sha256(package_key.encode())[:24]}"
    release_version = f"usda-fdc-foundation-{metadata.release}-{expected_selection_sha256[:12]}"
    manifest = {
        "contract_version": CONTRACT_VERSION,
        "package_kind": PACKAGE_KIND,
        "package_id": package_id,
        "producer": "nutrition-data-factory",
        "producer_version": PRODUCER_VERSION,
        "handoff_profile": HANDOFF_PROFILE,
        "source": {
            "source_code": metadata.code,
            "release": metadata.release,
            "published_date": metadata.release,
            "object_uri": metadata.locator,
            "artifact_sha256": extracted_artifact.sha256.lower(),
            "archive_sha256": archive_artifact.sha256.lower(),
        },
        "selection": {
            "selection_version": selection["compatibility_version"],
            "selection_sha256": expected_selection_sha256,
            "record_count": len(selected_records),
            "auto_add_records": False,
        },
        "policy_versions": {
            "handoff": CONTRACT_VERSION,
            "nutrient_crosswalk": NUTRIENT_CROSSWALK_POLICY_VERSION,
            "selection": selection["compatibility_version"],
        },
        "dataset_release": {"dataset_code": metadata.code, "version": metadata.release},
        "catalog_release_version": release_version,
        "files": file_entries,
        "import_mode": "staged_only",
        "production_eligible": False,
        "activation_authorized": False,
        "backend_baseline": backend_baseline,
    }
    _write_json(output_dir / "manifest.json", manifest)
    _write_checksums(output_dir, [entry["path"] for entry in file_entries])
    return output_dir


def _canonical_records(records: Iterable[FdcSourceRecord], metadata: SourceMetadata) -> list[dict[str, Any]]:
    output = []
    for record in records:
        if not isinstance(record, FdcSourceRecord):
            raise ValueError("catalog handoff requires parsed FDC source records")
        payload = record.payload
        if not isinstance(payload, dict):
            raise ValueError(f"FDC source record {record.fdc_id} payload must be an object")
        if payload.get("fdcId") != record.fdc_id or payload.get("dataType") != "Foundation":
            raise ValueError(f"FDC source record {record.fdc_id} payload identity is inconsistent")
        if not _SHA256.fullmatch(record.payload_sha256):
            raise ValueError(f"FDC source record {record.fdc_id} has an invalid payload hash")
        output.append({
            "source_code": metadata.code,
            "release": FDC_FOUNDATION_RELEASE,
            "source_id": str(record.fdc_id),
            "description": record.description,
            "data_type": record.data_type,
            "payload_sha256": record.payload_sha256.lower(),
            "payload": payload,
        })
    output.sort(key=lambda item: (item["source_code"], item["release"], int(item["source_id"])))
    return output


def _canonical_concepts(values: list[dict[str, Any]], records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    _require_source_entries(values, records, "food concept")
    _validate_unique([str(item.get("source_id", "")) for item in values], "food concept")
    by_id = {item.get("source_id"): item for item in values if isinstance(item, dict)}
    output = []
    for record in records:
        source_id = record["source_id"]
        source_code = record["source_code"]
        original = by_id.get(source_id, {})
        concept_id = f"source-food:{source_code}:{source_id}"
        output.append({
            "concept_id": concept_id,
            "semantic_key": f"usda-fdc:{source_id}",
            "entity_kind": "basic_food",
            "lifecycle_status": "candidate",
            "source_code": source_code,
            "source_id": source_id,
            "source_payload_sha256": record["payload_sha256"],
            "review_status": str(original.get("review_status", "proposal")),
        })
    return output


def _canonical_names(values: list[dict[str, Any]], records: list[dict[str, Any]], concepts: list[dict[str, Any]]) -> list[dict[str, Any]]:
    _require_source_entries(values, records, "food name")
    _validate_unique([str(item.get("source_id", "")) for item in values], "food name")
    by_id = {item.get("source_id"): item for item in values if isinstance(item, dict)}
    concepts_by_id = {item["source_id"]: item["concept_id"] for item in concepts}
    output = []
    for record in records:
        source_id = record["source_id"]
        source_code = record["source_code"]
        original = by_id.get(source_id, {})
        if original.get("name") is not None and str(original["name"]).strip() != record["description"]:
            raise ValueError(f"food name is not grounded in source description for {source_id}")
        name = record["description"]
        output.append({
            "name_id": f"source-name:{source_code}:{source_id}",
            "concept_id": concepts_by_id[source_id],
            "source_code": source_code,
            "source_id": source_id,
            "locale": "en-US",
            "name": name,
            "normalized_name": name.lower(),
            "name_type": "preferred",
            "source_payload_sha256": record["payload_sha256"],
            "status": "source_observed",
        })
    return output


def _canonical_mappings(values: list[dict[str, Any]], records: list[dict[str, Any]], concepts: list[dict[str, Any]]) -> list[dict[str, Any]]:
    _require_source_entries(values, records, "source mapping")
    _validate_unique([str(item.get("source_id", "")) for item in values], "source mapping")
    by_id = {item.get("source_id"): item for item in values if isinstance(item, dict)}
    concepts_by_id = {item["source_id"]: item["concept_id"] for item in concepts}
    output = []
    for record in records:
        source_id = record["source_id"]
        source_code = record["source_code"]
        original = by_id.get(source_id, {})
        output.append({
            "mapping_id": f"source-mapping:{source_code}:{source_id}",
            "source_code": source_code,
            "release": FDC_FOUNDATION_RELEASE,
            "source_id": source_id,
            "source_payload_sha256": record["payload_sha256"],
            "concept_id": concepts_by_id[source_id],
            "mapping_type": "exact",
            "mapping_method": "fdc_exact_external_id",
            "score": 1.0,
            "policy_version": str(original.get("policy_version", "catalog-handoff-1.0.0")),
            "review_status": "proposal",
            "rationale": "Deterministic exact mapping from the pinned FDC external ID; requires review before publication.",
        })
    return output


def _canonical_compositions(values: list[dict[str, Any]], records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    output = []
    seen = set()
    allowed_ids = {"1003", "1004", "1005", "2048", "2047"}
    raw_by_id = {item["source_id"]: item for item in records}
    for value in values:
        if not isinstance(value, dict) or value.get("target_code") not in SUPPORTED_NUTRIENT_CODES:
            continue
        source_id = str(value.get("source_id", ""))
        raw = raw_by_id.get(source_id)
        if raw is None:
            raise ValueError(f"composition value references unknown source {source_id}")
        if value.get("source_code", raw["source_code"]) != raw["source_code"]:
            raise ValueError(f"composition source identity mismatch for {source_id}")
        try:
            source_nutrient_id = int(value.get("source_nutrient_id", 0))
        except (TypeError, ValueError) as error:
            raise ValueError(f"invalid source nutrient ID for {source_id}") from error
        if str(source_nutrient_id) not in allowed_ids:
            continue
        target_code = str(value["target_code"])
        identity = (source_id, target_code)
        if identity in seen:
            raise ValueError(f"duplicate composition value for {source_id}/{target_code}")
        seen.add(identity)
        amount = value.get("value")
        if amount is not None and (isinstance(amount, bool) or not isinstance(amount, (int, float)) or not math.isfinite(float(amount)) or float(amount) < 0):
            raise ValueError(f"invalid composition value for {source_id}/{target_code}")
        expected_source_ids = {
            "protein_g": {1003},
            "fat_g": {1004},
            "carbohydrate_g": {1005},
            "energy_kcal": {2047, 2048},
        }
        expected_unit = "KCAL" if target_code == "energy_kcal" else "G"
        source_unit = str(value.get("source_unit", expected_unit)).upper()
        value_status = str(value.get("value_status", "numeric"))
        if source_nutrient_id not in expected_source_ids[target_code] or source_unit != expected_unit:
            raise ValueError(f"unsupported source nutrient/unit for {source_id}/{target_code}")
        observation = _source_nutrient_observation(raw, source_nutrient_id)
        nutrient = observation.get("nutrient")
        if not isinstance(nutrient, dict) or str(nutrient.get("unitName", "")).upper() != source_unit:
            raise ValueError(f"composition unit is not grounded in raw evidence for {source_id}/{target_code}")
        raw_amount = observation.get("amount")
        if raw_amount is None:
            if amount is not None or value_status != "missing":
                raise ValueError(f"composition missing state is not grounded for {source_id}/{target_code}")
        elif amount != raw_amount:
            raise ValueError(f"composition amount is not grounded in raw evidence for {source_id}/{target_code}")
        expected_status = "missing" if raw_amount is None else ("zero" if raw_amount == 0 else "numeric")
        if value_status != expected_status:
            raise ValueError(f"composition value status is not grounded for {source_id}/{target_code}")
        expected_label = str(nutrient.get("name", "")).strip()
        if expected_label and str(value.get("source_label", "")).strip() != expected_label:
            raise ValueError(f"composition label is not grounded in raw evidence for {source_id}/{target_code}")
        output.append({
            "value_id": f"composition:{raw['source_code']}:{source_id}:{target_code}",
            "source_code": raw["source_code"],
            "release": FDC_FOUNDATION_RELEASE,
            "source_id": source_id,
            "source_payload_sha256": raw["payload_sha256"],
            "target_code": target_code,
            "source_nutrient_id": source_nutrient_id,
            "source_label": str(value.get("source_label", target_code)),
            "source_unit": source_unit,
            "source_method": str(value.get("source_method", "declared_or_analytical")),
            "value": amount,
            "value_status": value_status,
            "conversion": "identity",
            "canonical_unit": "kcal" if target_code == "energy_kcal" else "g",
            "policy_version": str(value.get("policy_version", NUTRIENT_CROSSWALK_POLICY_VERSION)),
            "review_status": "proposal",
            "reviewer_decision_status": "pending_human_review",
        })
    output.sort(key=lambda item: (int(item["source_id"]), item["target_code"], item["source_nutrient_id"]))
    return output


def _source_nutrient_observation(record: dict[str, Any], source_nutrient_id: int) -> dict[str, Any]:
    observations = record["payload"].get("foodNutrients")
    if not isinstance(observations, list):
        raise ValueError(f"source record {record['source_id']} has no foodNutrients evidence")
    matches = [
        item for item in observations
        if isinstance(item, dict)
        and isinstance(item.get("nutrient"), dict)
        and item["nutrient"].get("id") == source_nutrient_id
    ]
    if len(matches) != 1:
        raise ValueError(f"source record {record['source_id']} has {len(matches)} observations for nutrient {source_nutrient_id}")
    return matches[0]


def _validate_profile_completeness(records: list[dict[str, Any]], compositions: list[dict[str, Any]]) -> None:
    by_source: dict[str, set[str]] = {record["source_id"]: set() for record in records}
    for value in compositions:
        by_source.setdefault(value["source_id"], set()).add(value["target_code"])
        if value["value_status"] not in {"numeric", "zero"} or value["value"] is None:
            raise ValueError(f"reviewed profile requires numeric core evidence for {value['source_id']}/{value['target_code']}")
    for source_id, target_codes in by_source.items():
        if target_codes != set(SUPPORTED_NUTRIENT_CODES):
            raise ValueError(f"reviewed profile requires all four core nutrients for {source_id}")
    if len(compositions) != len(records) * len(SUPPORTED_NUTRIENT_CODES):
        raise ValueError("reviewed profile composition count does not match selected records")


def _validate_references(records: list[dict[str, Any]], concepts: list[dict[str, Any]], names: list[dict[str, Any]], mappings: list[dict[str, Any]], compositions: list[dict[str, Any]]) -> None:
    source_keys = {(item["source_code"], item["release"], item["source_id"]) for item in records}
    concept_ids = {item["concept_id"] for item in concepts}
    _validate_unique([item["concept_id"] for item in concepts], "food concept")
    _validate_unique([item["name_id"] for item in names], "food name")
    _validate_unique([item["mapping_id"] for item in mappings], "source mapping")
    _validate_unique([item["value_id"] for item in compositions], "composition value")
    for item in names:
        if item["concept_id"] not in concept_ids:
            raise ValueError(f"food name references unknown concept {item['concept_id']}")
    for item in mappings:
        if (item["source_code"], item["release"], item["source_id"]) not in source_keys or item["concept_id"] not in concept_ids:
            raise ValueError(f"source mapping references unknown source or concept {item['mapping_id']}")
    for item in compositions:
        if (item["source_code"], item["release"], item["source_id"]) not in source_keys:
            raise ValueError(f"composition value references unknown source {item['value_id']}")


def _validate_unique(values: Iterable[str], label: str) -> None:
    values = list(values)
    if len(values) != len(set(values)):
        raise ValueError(f"duplicate {label} identity")


def _require_source_entries(values: list[dict[str, Any]], records: list[dict[str, Any]], label: str) -> None:
    if not values:
        return
    available = {str(item.get("source_id", "")) for item in values}
    missing = [record["source_id"] for record in records if record["source_id"] not in available]
    if missing:
        raise ValueError(f"{label} input has dangling/missing references: {','.join(missing)}")


def _assert_safe_relative_path(value: str) -> None:
    if not value or value.startswith(("/", "\\")) or ":" in value or ".." in value.split("/") or not _SAFE_PATH.fullmatch(value):
        raise ValueError(f"unsafe package path: {value}")


def _write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8", newline="\n")


def _write_jsonl(path: Path, values: Iterable[dict[str, Any]]) -> None:
    lines = [json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) for value in values]
    path.write_text("\n".join(lines) + ("\n" if lines else ""), encoding="utf-8", newline="\n")


def _write_checksums(output_dir: Path, payload_paths: Iterable[str]) -> None:
    paths = ["manifest.json", *sorted(payload_paths)]
    lines = [f"{_sha256((output_dir / path).read_bytes())}  {path}" for path in paths]
    (output_dir / "checksums.sha256").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()
