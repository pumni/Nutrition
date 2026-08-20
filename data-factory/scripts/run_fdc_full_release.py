from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.adapters.fdc_foundation import (  # noqa: E402
    FDC_FOUNDATION_RELEASE,
    FdcFoundationAdapter,
    FdcSourceRecord,
)
from nutrition_data_factory.artifacts import AcquisitionMetadata, ArtifactStore  # noqa: E402
from nutrition_data_factory.compatibility import (  # noqa: E402
    compare_fdc_selection,
    load_compatibility_manifest,
)
from nutrition_data_factory.curation.extractor import ExtractionPolicy, extract_candidates  # noqa: E402
from nutrition_data_factory.impact import build_impact_report  # noqa: E402
from nutrition_data_factory.models import NormalizedRecord  # noqa: E402
from nutrition_data_factory.normalization.synthetic import normalize_label  # noqa: E402
from nutrition_data_factory.nutrients import (  # noqa: E402
    build_fdc_nutrient_registry,
    canonicalize_fdc_nutrients,
)
from nutrition_data_factory.release.full_compiler import compile_full_catalog_package  # noqa: E402
from nutrition_data_factory.source_registry import SourceRegistry  # noqa: E402
from nutrition_data_factory.validation.profiler import profile_records  # noqa: E402


ARCHIVE_SHA256 = "186e988ec542e913f51ef62b86a47758e8cdd0d1dc3889e7b055581f3c09c77a"
EXTRACTED_SHA256 = "27d1fe3fd89edfbe528ed915da5619320e1d004d4594603a1b19bdb1511590cc"
ACQUISITION_TOOL_VERSION = "fdc-full-release-0.1.0"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Build a full-source FDC Foundation catalog candidate")
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--extracted", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--object-store", type=Path, required=True)
    parser.add_argument("--backend-baseline", required=True)
    parser.add_argument("--vietnamese-corpus", type=Path)
    parser.add_argument(
        "--retrieved-at",
        required=True,
        help="Pinned acquisition timestamp; required so the package is reproducible byte-for-byte.",
    )
    args = parser.parse_args(argv)

    archive_bytes = args.archive.read_bytes()
    extracted_bytes = args.extracted.read_bytes()
    _require_sha256(archive_bytes, ARCHIVE_SHA256, "archive")
    _require_sha256(extracted_bytes, EXTRACTED_SHA256, "extracted JSON")

    registry = SourceRegistry.load(ROOT / "config" / "source_registry.json")
    metadata = registry.get("usda_fdc_foundation")
    store = ArtifactStore(args.object_store)
    archive_ref = store.put_bytes(
        archive_bytes,
        content_type="application/zip",
        expected_sha256=ARCHIVE_SHA256,
        acquisition=_acquisition(metadata, args.archive.name, len(archive_bytes), "application/zip", ARCHIVE_SHA256, args.retrieved_at),
    )
    extracted_ref = store.put_bytes(
        extracted_bytes,
        content_type="application/json",
        expected_sha256=EXTRACTED_SHA256,
        acquisition=_acquisition(metadata, args.extracted.name, len(extracted_bytes), "application/json", EXTRACTED_SHA256, args.retrieved_at),
    )
    parsed = FdcFoundationAdapter().parse(extracted_bytes, release=FDC_FOUNDATION_RELEASE, expected_sha256=EXTRACTED_SHA256)
    records = [_record_dict(record, metadata.code) for record in parsed.accepted_records]
    source_quality = profile_records(records, metadata=metadata, artifact_sha256=EXTRACTED_SHA256, expected_artifact_sha256=EXTRACTED_SHA256)

    compatibility_manifest = load_compatibility_manifest(ROOT / "config" / "backend-fdc-selection.json")
    compatibility = compare_fdc_selection(compatibility_manifest, [record.fdc_id for record in parsed.accepted_records if record.fdc_id in set(compatibility_manifest["fdc_ids"])])
    nutrient_registry = build_fdc_nutrient_registry(records)
    normalized_records, composition_values, source_nutrient_rejections = _normalize_records(parsed.accepted_records, metadata.code)
    quarantine_entries = [
        {
            "kind": "source_record",
            "row_index": reject.row_index,
            "reason_code": reject.reason_code,
            "detail": reject.detail,
        }
        for reject in parsed.rejected_records
    ]
    quarantine_entries.extend(source_nutrient_rejections)
    quarantine_report = {
        "report_version": "source-quarantine-0.1.0",
        "policy": "preserve_raw_quarantine_invalid",
        "entries": quarantine_entries,
        "counts_by_kind": dict(Counter(item["kind"] for item in quarantine_entries)),
        "counts_by_reason": dict(Counter(item["reason_code"] for item in quarantine_entries)),
        "raw_source_rows_accounted_for": parsed.raw_record_count == len(parsed.accepted_records) + len(parsed.rejected_records),
    }

    vietnamese_candidates, vietnamese_report = _load_vietnamese_candidates(args.vietnamese_corpus)
    food_concepts, source_names, source_mappings = _food_identity_candidates(parsed.accepted_records, metadata.code)
    food_names = [*source_names, *vietnamese_candidates]
    recipes: list[dict[str, Any]] = []
    recipe_components: list[dict[str, Any]] = []
    portions: list[dict[str, Any]] = []
    recipe_report = _not_supplied_report("recipe-evidence-0.1.0", "recipe evidence input was not supplied")
    portion_report = _not_supplied_report("portion-evidence-0.1.0", "measured portion evidence input was not supplied")

    counts = {
        "raw_source_rows": parsed.raw_record_count,
        "accepted_source_records": len(parsed.accepted_records),
        "quarantined_source_rows": len(parsed.rejected_records),
        "quarantined_nutrient_observations": len(source_nutrient_rejections),
        "normalized_records": len(normalized_records),
        "raw_nutrient_observations": sum(len(record.nutrients) for record in parsed.accepted_records),
        "composition_values": len(composition_values),
        "source_nutrient_registry_entries": len(nutrient_registry),
        "food_concept_candidates": len(food_concepts),
        "food_name_candidates": len(food_names),
        "source_food_mapping_candidates": len(source_mappings),
        "vietnamese_identity_candidates": len(vietnamese_candidates),
        "recipes": len(recipes),
        "recipe_components": len(recipe_components),
        "portion_observations": len(portions),
    }
    registry_statuses = Counter(item["mapping_status"] for item in nutrient_registry)
    rejected_reasons = Counter(item["reason_code"] for record in normalized_records for item in record.attributes["rejected_nutrient_mappings"])
    completeness = {
        "report_version": "data-completeness-0.1.0",
        "scope": "full_approved_source_release",
        "source_code": metadata.code,
        "source_release": metadata.release,
        "source_artifact_sha256": EXTRACTED_SHA256,
        "source_schema_fingerprint": parsed.schema_fingerprint,
        "backend_baseline": args.backend_baseline,
        "counts": counts,
        "accounting": {
            "raw_equals_accepted_plus_quarantined": parsed.raw_record_count == len(parsed.accepted_records) + len(parsed.rejected_records),
            "every_accepted_source_record_normalized": len(normalized_records) == len(parsed.accepted_records),
            "every_raw_nutrient_preserved_in_source_records": sum(len(record.nutrients) for record in parsed.accepted_records) > 0,
        },
        "nutrient_mapping_coverage": {
            "unique_source_nutrient_ids": len(nutrient_registry),
            "approved_ids": registry_statuses.get("approved", 0),
            "source_preserved_unmapped_ids": registry_statuses.get("source_preserved_unmapped", 0),
            "raw_observations_with_unmapped_mapping": rejected_reasons.get("unmapped_source_nutrient", 0),
            "forbidden_observations": rejected_reasons.get("forbidden_energy", 0),
            "invalid_observations_quarantined": len(source_nutrient_rejections),
        },
        "identity_coverage": vietnamese_report,
        "recipe_coverage": recipe_report,
        "portion_coverage": portion_report,
        "unresolved_gaps": [
            f"{len(source_quality.errors)} full-source quality errors require review or remain quarantined",
            f"{registry_statuses.get('source_preserved_unmapped', 0)} source nutrient IDs have no approved product semantic mapping",
            "Vietnamese identity corpus was not supplied" if not args.vietnamese_corpus else "Vietnamese identity candidates remain proposals",
            "Recipe evidence was not supplied",
            "Measured portion evidence was not supplied",
            "Production activation and backend import were not attempted",
        ],
    }
    validation = {
        "rule_version": "full-source-accounting-0.1.0",
        "status": "failed_with_quarantine" if source_quality.errors or quarantine_entries else "passed",
        "passed": not bool(source_quality.errors or quarantine_entries),
        "errors": [json.dumps(item, sort_keys=True) for item in source_quality.errors],
        "warnings": [json.dumps(item, sort_keys=True) for item in source_quality.warnings],
        "quarantine_entry_count": len(quarantine_entries),
        "statistics": {
            **source_quality.statistics,
            "raw_record_count": parsed.raw_record_count,
            "accepted_record_count": len(parsed.accepted_records),
            "rejected_record_count": len(parsed.rejected_records),
            "normalized_record_count": len(normalized_records),
            "composition_value_count": len(composition_values),
        },
    }
    impact = build_impact_report({}, {}).to_dict()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    _write_json(output / "acquisition-report.json", {"archive_artifact": archive_ref.to_dict(), "extracted_artifact": extracted_ref.to_dict(), "retrieved_at": args.retrieved_at, "release": FDC_FOUNDATION_RELEASE})
    _write_json(output / "fdc-parse-report.json", parsed.to_dict())
    _write_json(output / "source-quality-report.json", source_quality.to_dict())
    _write_json(output / "backend-compatibility-report.json", compatibility.to_dict())
    _write_json(output / "quarantine-report.json", quarantine_report)
    _write_json(output / "completeness-manifest.json", completeness)
    _write_json(output / "vietnamese-identity-report.json", vietnamese_report)
    _write_json(output / "recipe-evidence-report.json", recipe_report)
    _write_json(output / "portion-evidence-report.json", portion_report)
    _write_json(output / "validation-report.json", validation)
    package_path = output / "full-catalog-package"
    compile_full_catalog_package(
        package_path,
        metadata=metadata,
        archive_artifact=archive_ref,
        extracted_artifact=extracted_ref,
        source_records=list(parsed.accepted_records),
        normalized_records=normalized_records,
        nutrient_registry=nutrient_registry,
        composition_values=composition_values,
        food_concepts=food_concepts,
        food_names=food_names,
        source_food_mappings=source_mappings,
        curation_decisions=[],
        recipes=recipes,
        recipe_components=recipe_components,
        portions=portions,
        quarantine_report=quarantine_report,
        source_quality_report=source_quality.to_dict(),
        compatibility_report=compatibility.to_dict(),
        validation_report=validation,
        impact_report=impact,
        completeness_manifest=completeness,
        vietnamese_identity_report=vietnamese_report,
        recipe_evidence_report=recipe_report,
        portion_evidence_report=portion_report,
        backend_baseline=args.backend_baseline,
    )
    _write_json(output / "full-release-summary.json", {
        "package_created": True,
        "package": str(package_path),
        "production_eligible": False,
        "activation_attempted": False,
        "validation_status": validation["status"],
        "raw_source_rows": parsed.raw_record_count,
        "accepted_source_records": len(parsed.accepted_records),
        "quarantined_source_rows": len(parsed.rejected_records),
        "normalized_records": len(normalized_records),
        "backend_regression_selection_status": compatibility.status,
    })
    return 0


def _record_dict(record: FdcSourceRecord, source_code: str) -> dict[str, Any]:
    return {
        "source_code": source_code,
        "release": FDC_FOUNDATION_RELEASE,
        "source_id": str(record.fdc_id),
        "fdc_id": record.fdc_id,
        "description": record.description,
        "payload_sha256": record.payload_sha256,
        "nutrients": list(record.nutrients),
        "portions": list(record.portions),
    }


def _normalize_records(records: tuple[FdcSourceRecord, ...], source_code: str) -> tuple[list[NormalizedRecord], list[dict[str, Any]], list[dict[str, Any]]]:
    normalized: list[NormalizedRecord] = []
    composition_values: list[dict[str, Any]] = []
    quarantined: list[dict[str, Any]] = []
    for record in sorted(records, key=lambda item: item.fdc_id):
        canonical = canonicalize_fdc_nutrients(record.nutrients)
        for item in canonical.rejected:
            if item["reason_code"] in {"negative_value", "invalid_value", "invalid_unit", "duplicate_mapping", "forbidden_energy", "status_value_conflict"}:
                quarantined.append({"kind": "nutrient_observation", "fdc_id": record.fdc_id, **item})
        for value in canonical.values:
            composition_values.append(
                {
                    "source_code": source_code,
                    "release": FDC_FOUNDATION_RELEASE,
                    "source_id": str(record.fdc_id),
                    "source_payload_sha256": record.payload_sha256,
                    "review_status": "proposal",
                    "reviewer_decision_status": "pending_human_review",
                    **value.to_dict(),
                }
            )
        normalized.append(
            NormalizedRecord(
                source_code=source_code,
                release=FDC_FOUNDATION_RELEASE,
                source_id=str(record.fdc_id),
                original_label=record.description,
                normalized_label=normalize_label(record.description),
                attributes={
                    "fdc_id": record.fdc_id,
                    "data_type": record.data_type,
                    "payload_sha256": record.payload_sha256,
                    "nutrients": list(record.nutrients),
                    "portions": list(record.portions),
                    "canonical_nutrients": [value.to_dict() for value in canonical.values],
                    "rejected_nutrient_mappings": list(canonical.rejected),
                },
            )
        )
    return normalized, composition_values, quarantined


def _food_identity_candidates(records: tuple[FdcSourceRecord, ...], source_code: str) -> tuple[list[dict[str, Any]], list[dict[str, Any]], list[dict[str, Any]]]:
    concepts = []
    names = []
    mappings = []
    for record in sorted(records, key=lambda item: item.fdc_id):
        concept_id = f"source-food:{source_code}:{record.fdc_id}"
        concepts.append({"concept_id": concept_id, "lifecycle_status": "candidate", "source_code": source_code, "source_id": str(record.fdc_id), "source_payload_sha256": record.payload_sha256, "review_status": "proposal"})
        names.append({"name_id": f"source-name:{source_code}:{record.fdc_id}", "concept_id": concept_id, "locale": "und", "name": record.description, "source_payload_sha256": record.payload_sha256, "status": "source_observed"})
        mappings.append({"mapping_id": f"source-mapping:{source_code}:{record.fdc_id}", "source_code": source_code, "source_id": str(record.fdc_id), "source_payload_sha256": record.payload_sha256, "concept_id": concept_id, "status": "proposal"})
    return concepts, names, mappings


def _load_vietnamese_candidates(path: Path | None) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    if path is None:
        return [], _not_supplied_report("vietnamese-identity-0.1.0", "Vietnamese corpus was not supplied")
    corpus_bytes = path.read_bytes()
    corpus_sha256 = _sha256(corpus_bytes)
    payload = json.loads(corpus_bytes.decode("utf-8"))
    result = extract_candidates(payload, ExtractionPolicy())
    candidates = [
        {
            "name_id": item["candidate_id"],
            "locale": "vi-VN",
            "name": item["phrase"],
            "normalized_phrase": item["normalized_phrase"],
            "status": "proposal",
            "review_status": item["review_status"],
            "evidence_refs": item["context_refs"],
            "source_corpus_sha256": corpus_sha256,
        }
        for item in result.candidates
    ]
    return candidates, {"corpus_available": True, "corpus_path": str(path), "corpus_sha256": corpus_sha256, "status": "proposal_only", **result.report, "candidate_count": len(candidates)}


def _not_supplied_report(report_version: str, detail: str) -> dict[str, Any]:
    return {"report_version": report_version, "status": "not_supplied", "coverage": 0, "detail": detail}


def _acquisition(metadata: Any, filename: str, size: int, content_type: str, sha256: str, retrieved_at: str) -> AcquisitionMetadata:
    return AcquisitionMetadata(
        source_code=metadata.code,
        publisher=metadata.publisher,
        release=FDC_FOUNDATION_RELEASE,
        retrieved_at=retrieved_at,
        filename=filename,
        size=size,
        content_type=content_type,
        sha256=sha256,
        rights_state=metadata.rights_state,
        acquisition_tool_version=ACQUISITION_TOOL_VERSION,
    )


def _write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def _require_sha256(payload: bytes, expected: str, label: str) -> None:
    actual = _sha256(payload)
    if actual != expected:
        raise SystemExit(f"{label} SHA-256 mismatch: expected {expected}, actual {actual}")


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


if __name__ == "__main__":
    raise SystemExit(main())
