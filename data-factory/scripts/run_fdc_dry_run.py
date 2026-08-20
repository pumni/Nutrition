from __future__ import annotations

import argparse
import hashlib
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.adapters.fdc_foundation import (  # noqa: E402
    FDC_FOUNDATION_RELEASE,
    FdcFoundationAdapter,
)
from nutrition_data_factory.artifacts import AcquisitionMetadata, ArtifactStore  # noqa: E402
from nutrition_data_factory.compatibility import (  # noqa: E402
    compare_fdc_selection,
    load_compatibility_manifest,
)
from nutrition_data_factory.curation.queue import build_candidate_queue  # noqa: E402
from nutrition_data_factory.impact import build_impact_report  # noqa: E402
from nutrition_data_factory.models import NormalizedRecord, ValidationReport  # noqa: E402
from nutrition_data_factory.nutrients import canonicalize_fdc_nutrients  # noqa: E402
from nutrition_data_factory.normalization.synthetic import normalize_label  # noqa: E402
from nutrition_data_factory.release.compiler import compile_candidate_package  # noqa: E402
from nutrition_data_factory.source_registry import SourceRegistry  # noqa: E402
from nutrition_data_factory.validation.profiler import profile_records  # noqa: E402


ARCHIVE_SHA256 = "186e988ec542e913f51ef62b86a47758e8cdd0d1dc3889e7b055581f3c09c77a"
EXTRACTED_SHA256 = "27d1fe3fd89edfbe528ed915da5619320e1d004d4594603a1b19bdb1511590cc"
ACQUISITION_TOOL_VERSION = "fdc-dry-run-0.1.0"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Run the pinned FDC Foundation offline dry-run")
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--extracted", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--object-store", type=Path, required=True)
    parser.add_argument("--retrieved-at", default=datetime.now(timezone.utc).isoformat())
    args = parser.parse_args(argv)

    archive_bytes = args.archive.read_bytes()
    extracted_bytes = args.extracted.read_bytes()
    if _sha256(archive_bytes) != ARCHIVE_SHA256:
        raise SystemExit("archive SHA-256 does not match the pinned April 2026 release")
    if _sha256(extracted_bytes) != EXTRACTED_SHA256:
        raise SystemExit("extracted JSON SHA-256 does not match the pinned April 2026 release")
    registry = SourceRegistry.load(ROOT / "config" / "source_registry.json")
    metadata = registry.get("usda_fdc_foundation")
    store = ArtifactStore(args.object_store)
    archive_ref = store.put_bytes(
        archive_bytes,
        content_type="application/zip",
        expected_sha256=ARCHIVE_SHA256,
        acquisition=AcquisitionMetadata(
            source_code=metadata.code,
            publisher=metadata.publisher,
            release=FDC_FOUNDATION_RELEASE,
            retrieved_at=args.retrieved_at,
            filename=args.archive.name,
            size=len(archive_bytes),
            content_type="application/zip",
            sha256=ARCHIVE_SHA256,
            rights_state=metadata.rights_state,
            acquisition_tool_version=ACQUISITION_TOOL_VERSION,
        ),
    )
    extracted_ref = store.put_bytes(
        extracted_bytes,
        content_type="application/json",
        expected_sha256=EXTRACTED_SHA256,
        acquisition=AcquisitionMetadata(
            source_code=metadata.code,
            publisher=metadata.publisher,
            release=FDC_FOUNDATION_RELEASE,
            retrieved_at=args.retrieved_at,
            filename=args.extracted.name,
            size=len(extracted_bytes),
            content_type="application/json",
            sha256=EXTRACTED_SHA256,
            rights_state=metadata.rights_state,
            acquisition_tool_version=ACQUISITION_TOOL_VERSION,
        ),
    )
    parsed = FdcFoundationAdapter().parse(
        extracted_bytes,
        release=FDC_FOUNDATION_RELEASE,
        expected_sha256=EXTRACTED_SHA256,
    )
    records = [
        {
            "source_code": metadata.code,
            "release": FDC_FOUNDATION_RELEASE,
            "source_id": str(record.fdc_id),
            "fdc_id": record.fdc_id,
            "description": record.description,
            "payload_sha256": record.payload_sha256,
            "nutrients": list(record.nutrients),
            "portions": list(record.portions),
        }
        for record in parsed.accepted_records
    ]
    quality = profile_records(
        records,
        metadata=metadata,
        artifact_sha256=EXTRACTED_SHA256,
        expected_artifact_sha256=EXTRACTED_SHA256,
    )
    compatibility_manifest = load_compatibility_manifest(ROOT / "config" / "backend-fdc-selection.json")
    expected_ids = set(compatibility_manifest["fdc_ids"])
    selected_source_records = [record for record in parsed.accepted_records if record.fdc_id in expected_ids]
    compatibility = compare_fdc_selection(
        compatibility_manifest,
        [record.fdc_id for record in selected_source_records],
    )
    normalized_records: list[NormalizedRecord] = []
    crosswalk_reports = []
    for record in sorted(selected_source_records, key=lambda item: item.fdc_id):
        canonical = canonicalize_fdc_nutrients(record.nutrients)
        crosswalk_reports.append(
            {
                "fdc_id": record.fdc_id,
                "valid": canonical.valid,
                "values": [value.to_dict() for value in canonical.values],
                "rejected": list(canonical.rejected),
            }
        )
        normalized_records.append(
            NormalizedRecord(
                source_code=metadata.code,
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
    curation_queue = build_candidate_queue(normalized_records)
    validation = ValidationReport(
        rule_version=quality.rule_version,
        passed=quality.passed and compatibility.matches and all(item["valid"] for item in crosswalk_reports),
        errors=tuple(
            json.dumps(item, sort_keys=True)
            for item in [*quality.errors, *([] if compatibility.matches else [{"reason_code": "selection_mismatch"}])]
            + [
                {"reason_code": "nutrient_crosswalk", "fdc_id": item["fdc_id"]}
                for item in crosswalk_reports
                if not item["valid"]
            ]
        ),
        warnings=tuple(json.dumps(item, sort_keys=True) for item in quality.warnings),
        statistics={
            **quality.statistics,
            "raw_record_count": parsed.raw_record_count,
            "accepted_record_count": len(parsed.accepted_records),
            "rejected_record_count": len(parsed.rejected_records),
            "selected_record_count": len(selected_source_records),
        },
    )
    impact = build_impact_report({}, {}).to_dict()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    _write_json(
        output / "acquisition-report.json",
        {
            "archive_artifact": archive_ref.to_dict(),
            "extracted_artifact": extracted_ref.to_dict(),
            "retrieved_at": args.retrieved_at,
            "release": FDC_FOUNDATION_RELEASE,
        },
    )
    _write_json(output / "fdc-parse-report.json", parsed.to_dict())
    _write_json(output / "quality-report.json", quality.to_dict())
    _write_json(output / "compatibility-report.json", compatibility.to_dict())
    _write_json(output / "nutrient-crosswalk-report.json", {"records": crosswalk_reports})
    _write_json(output / "validation-report.json", validation.to_dict())
    _write_json(output / "dry-run-summary.json", {
        "production_eligible": False,
        "activation_attempted": False,
        "candidate_package_created": False,
    })
    package_path = output / "candidate-package"
    if validation.passed:
        compile_candidate_package(
            package_path,
            metadata,
            extracted_ref,
            normalized_records,
            curation_queue,
            validation,
            impact_report=impact,
            production_candidate=False,
            production_eligible=False,
        )
        _write_json(output / "dry-run-summary.json", {
            "production_eligible": False,
            "activation_attempted": False,
            "candidate_package_created": True,
            "candidate_package": str(package_path),
        })
    return 0 if validation.passed else 1


def _write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


if __name__ == "__main__":
    raise SystemExit(main())

