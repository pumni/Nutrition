from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any, Iterable

from ..artifacts import ArtifactRef
from ..models import NormalizedRecord, SourceMetadata, ValidationReport
from ..impact import IMPACT_REPORT_VERSION
from ..source_registry import production_rights_error


GENERATOR_VERSION = "scaffold-0.2.0"


class ReleaseCompilationError(RuntimeError):
    pass


def compile_candidate_package(
    output_dir: Path,
    metadata: SourceMetadata,
    artifact: ArtifactRef,
    records: list[NormalizedRecord],
    curation_queue: list[dict[str, Any]],
    validation: ValidationReport,
    production_candidate: bool = False,
    impact_report: dict[str, Any] | None = None,
    production_eligible: bool = False,
    owner_approval_ref: str | None = None,
) -> Path:
    if not validation.passed:
        raise ReleaseCompilationError("cannot compile a package with validation errors")
    if production_eligible and not owner_approval_ref:
        raise ReleaseCompilationError("production_eligible requires external owner approval reference")
    if production_candidate or production_eligible:
        rights_error = production_rights_error(metadata)
        if rights_error is not None:
            raise ReleaseCompilationError(rights_error)
    output_dir = output_dir.resolve()
    if output_dir.exists() and any(output_dir.iterdir()):
        raise ReleaseCompilationError(f"release output is not empty: {output_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)

    normalized_path = output_dir / "normalized-records.jsonl"
    curation_path = output_dir / "curation-queue.jsonl"
    source_path = output_dir / "source-releases.json"
    validation_path = output_dir / "validation-report.json"
    impact_path = output_dir / "impact-report.json"
    effective_impact = impact_report or {
        "report_version": IMPACT_REPORT_VERSION,
        "status": "not_requested",
        "changes": {},
        "nutrient_deltas": [],
        "coverage": {},
        "fixed_meal_impact": [],
        "material_delta_flags": [],
        "clinical_correctness_claimed": False,
    }
    _write_jsonl(normalized_path, (record.to_dict() for record in records))
    _write_jsonl(curation_path, curation_queue)
    _write_json(
        source_path,
        {
            "sources": [
                {
                    **metadata.to_dict(),
                    "artifact": artifact.to_dict(),
                }
            ]
        },
    )
    _write_json(validation_path, validation.to_dict())
    _write_json(impact_path, effective_impact)

    manifest = {
        "schema_version": "catalog-release-scaffold-0.2.0",
        "catalog_version": "synthetic-scaffold-0.1.0",
        "generator_version": GENERATOR_VERSION,
        "source_releases": [
            {
                "source_code": metadata.code,
                "release": metadata.release,
                "artifact_sha256": artifact.sha256,
            }
        ],
        "policy_versions": {
            "normalization": "synthetic-normalization-0.1.0",
            "validation": validation.rule_version,
            "source_rights": "source-registry-0.1.0",
            "impact": effective_impact.get("report_version", IMPACT_REPORT_VERSION),
        },
        "record_counts": {
            "normalized_records": len(records),
            "curation_proposals": len(curation_queue),
        },
        "validation_report_sha256": _sha256(validation_path.read_bytes()),
        "impact_report_sha256": _sha256(impact_path.read_bytes()),
        "production_eligible": production_eligible,
    }
    if owner_approval_ref is not None:
        manifest["review_approval_refs"] = [owner_approval_ref]
    _write_json(output_dir / "manifest.json", manifest)
    _write_checksums(output_dir)
    return output_dir


def _write_json(path: Path, value: Any) -> None:
    path.write_text(
        json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n",
        encoding="utf-8",
    )


def _write_jsonl(path: Path, values: Iterable[Any]) -> None:
    lines = [json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) for value in values]
    path.write_text("\n".join(lines) + ("\n" if lines else ""), encoding="utf-8")


def _write_checksums(output_dir: Path) -> None:
    entries = []
    for path in sorted(output_dir.iterdir(), key=lambda item: item.name):
        if path.name == "checksums.sha256" or not path.is_file():
            continue
        entries.append(f"{_sha256(path.read_bytes())}  {path.name}")
    (output_dir / "checksums.sha256").write_text("\n".join(entries) + "\n", encoding="utf-8")


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()
