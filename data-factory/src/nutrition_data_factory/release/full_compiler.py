from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any, Iterable

from ..adapters.fdc_foundation import FdcSourceRecord
from ..artifacts import ArtifactRef
from ..models import NormalizedRecord, SourceMetadata


FULL_PACKAGE_VERSION = "catalog-release-package-0.3.0"
FULL_GENERATOR_VERSION = "nutrition-data-factory-full-0.1.0"


def compile_full_catalog_package(
    output_dir: Path,
    *,
    metadata: SourceMetadata,
    archive_artifact: ArtifactRef,
    extracted_artifact: ArtifactRef,
    source_records: list[FdcSourceRecord],
    normalized_records: list[NormalizedRecord],
    nutrient_registry: list[dict[str, Any]],
    composition_values: list[dict[str, Any]],
    food_concepts: list[dict[str, Any]],
    food_names: list[dict[str, Any]],
    source_food_mappings: list[dict[str, Any]],
    vietnamese_review_packets: list[dict[str, Any]],
    recipe_evidence_candidates: list[dict[str, Any]],
    portion_evidence_candidates: list[dict[str, Any]],
    curation_decisions: list[dict[str, Any]],
    recipes: list[dict[str, Any]],
    recipe_components: list[dict[str, Any]],
    portions: list[dict[str, Any]],
    quarantine_report: dict[str, Any],
    source_quality_report: dict[str, Any],
    compatibility_report: dict[str, Any],
    validation_report: dict[str, Any],
    impact_report: dict[str, Any],
    completeness_manifest: dict[str, Any],
    vietnamese_identity_report: dict[str, Any],
    recipe_evidence_report: dict[str, Any],
    portion_evidence_report: dict[str, Any],
    backend_baseline: str,
) -> Path:
    output_dir = output_dir.resolve()
    if output_dir.exists() and any(output_dir.iterdir()):
        raise ValueError(f"full catalog output is not empty: {output_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)

    _write_jsonl(output_dir / "raw-source-records.jsonl", (record.to_dict() for record in source_records))
    _write_jsonl(output_dir / "normalized-records.jsonl", (record.to_dict() for record in normalized_records))
    _write_jsonl(output_dir / "nutrient-registry.jsonl", nutrient_registry)
    _write_jsonl(output_dir / "composition-values.jsonl", composition_values)
    _write_jsonl(output_dir / "food-concepts-candidates.jsonl", food_concepts)
    _write_jsonl(output_dir / "food-names-candidates.jsonl", food_names)
    _write_jsonl(output_dir / "source-food-mappings-candidates.jsonl", source_food_mappings)
    _write_jsonl(output_dir / "vietnamese-review-packets.jsonl", vietnamese_review_packets)
    _write_jsonl(output_dir / "recipe-evidence-candidates.jsonl", recipe_evidence_candidates)
    _write_jsonl(output_dir / "portion-evidence-candidates.jsonl", portion_evidence_candidates)
    _write_jsonl(output_dir / "curation-decisions.jsonl", curation_decisions)
    _write_jsonl(output_dir / "recipes.jsonl", recipes)
    _write_jsonl(output_dir / "recipe-components.jsonl", recipe_components)
    _write_jsonl(output_dir / "portion-observations.jsonl", portions)
    _write_json(
        output_dir / "source-releases.json",
        {
            "sources": [
                {
                    **metadata.to_dict(),
                    "archive_artifact": archive_artifact.to_dict(),
                    "extracted_artifact": extracted_artifact.to_dict(),
                }
            ]
        },
    )
    _write_json(
        output_dir / "dataset-release.json",
        {
            "dataset_code": metadata.code,
            "release": metadata.release,
            "status": "staged_candidate",
            "artifact_sha256": extracted_artifact.sha256,
            "archive_sha256": archive_artifact.sha256,
            "schema_fingerprint": completeness_manifest.get("source_schema_fingerprint"),
            "source_rights_state": metadata.rights_state,
            "production_eligible": False,
        },
    )
    _write_json(
        output_dir / "backend-import-manifest.json",
        {
            "schema_version": "backend-import-package-0.1.0",
            "backend_repository": "pumni/Nutrition_backend",
            "backend_baseline": backend_baseline,
            "import_mode": "staged_only",
            "activation_attempted": False,
            "production_eligible": False,
            "raw_provenance_preserved": True,
            "candidate_files": [
                "dataset-release.json",
                "source-releases.json",
                "raw-source-records.jsonl",
                "nutrient-registry.jsonl",
                "composition-values.jsonl",
                "food-concepts-candidates.jsonl",
                "food-names-candidates.jsonl",
                "source-food-mappings-candidates.jsonl",
                "vietnamese-review-packets.jsonl",
                "curation-decisions.jsonl",
                "recipe-evidence-candidates.jsonl",
                "recipes.jsonl",
                "recipe-components.jsonl",
                "portion-evidence-candidates.jsonl",
                "portion-observations.jsonl",
                "source-quality-report.json",
                "backend-compatibility-report.json",
                "validation-report.json",
                "quarantine-report.json",
                "completeness-manifest.json",
                "vietnamese-identity-report.json",
                "recipe-evidence-report.json",
                "portion-evidence-report.json",
                "checksums.sha256",
            ],
        },
    )
    _write_json(output_dir / "quarantine-report.json", quarantine_report)
    _write_json(output_dir / "source-quality-report.json", source_quality_report)
    _write_json(output_dir / "backend-compatibility-report.json", compatibility_report)
    _write_json(output_dir / "validation-report.json", validation_report)
    _write_json(output_dir / "impact-report.json", impact_report)
    _write_json(output_dir / "completeness-manifest.json", completeness_manifest)
    _write_json(output_dir / "vietnamese-identity-report.json", vietnamese_identity_report)
    _write_json(output_dir / "recipe-evidence-report.json", recipe_evidence_report)
    _write_json(output_dir / "portion-evidence-report.json", portion_evidence_report)

    manifest = {
        "schema_version": FULL_PACKAGE_VERSION,
        "generator_version": FULL_GENERATOR_VERSION,
        "release_status": "candidate_review_required",
        "source_releases": [
            {
                "source_code": metadata.code,
                "release": metadata.release,
                "archive_sha256": archive_artifact.sha256,
                "extracted_sha256": extracted_artifact.sha256,
            }
        ],
        "backend_baseline": backend_baseline,
        "policy_versions": {
            "nutrient_crosswalk": _policy_version(nutrient_registry),
            "validation": validation_report.get("rule_version"),
            "source_rights": "source-registry-0.1.0",
            "completeness": completeness_manifest.get("report_version"),
        },
        "record_counts": completeness_manifest.get("counts", {}),
        "regression_selection": {
            "selection_sha256": compatibility_report.get("expected_selection_sha256"),
            "status": compatibility_report.get("status"),
        },
        "quarantine_required": bool(quarantine_report.get("entries")),
        "production_eligible": False,
        "activation_attempted": False,
        "validation_report_sha256": _sha256((output_dir / "validation-report.json").read_bytes()),
        "source_quality_report_sha256": _sha256((output_dir / "source-quality-report.json").read_bytes()),
        "quarantine_report_sha256": _sha256((output_dir / "quarantine-report.json").read_bytes()),
        "completeness_manifest_sha256": _sha256((output_dir / "completeness-manifest.json").read_bytes()),
        "impact_report_sha256": _sha256((output_dir / "impact-report.json").read_bytes()),
        "evidence_report_sha256": {
            "vietnamese_identity": _sha256((output_dir / "vietnamese-identity-report.json").read_bytes()),
            "recipes": _sha256((output_dir / "recipe-evidence-report.json").read_bytes()),
            "portions": _sha256((output_dir / "portion-evidence-report.json").read_bytes()),
        },
    }
    _write_json(output_dir / "manifest.json", manifest)
    _write_checksums(output_dir)
    return output_dir


def _policy_version(registry: list[dict[str, Any]]) -> str | None:
    versions = {item.get("policy_version") for item in registry if item.get("policy_version")}
    return sorted(versions)[0] if len(versions) == 1 else None


def _write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")


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
