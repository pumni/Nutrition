from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


RELEASE_GATE_VERSION = "candidate-release-gate-0.1.0"


@dataclass(frozen=True)
class GateCheck:
    gate_id: str
    status: str
    evidence: dict[str, Any]
    detail: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "gate_id": self.gate_id,
            "status": self.status,
            "evidence": self.evidence,
            "detail": self.detail,
        }


def build_release_gate_report(
    package_dir: Path,
    run_dir: Path,
    compatibility_manifest: Path,
    backend_head: str | None = None,
) -> dict[str, Any]:
    package_dir = package_dir.resolve()
    run_dir = run_dir.resolve()
    manifest = _read_json(package_dir / "manifest.json")
    source_releases = _read_json(package_dir / "source-releases.json")
    validation = _read_json(package_dir / "validation-report.json")
    impact = _read_json(package_dir / "impact-report.json")
    source_quality = _read_json(run_dir / "source-quality-report.json")
    candidate_quality = _read_json(run_dir / "quality-report.json")
    compatibility = _read_json(run_dir / "compatibility-report.json")
    acquisition = _read_json(run_dir / "acquisition-report.json")
    selection = _read_json(compatibility_manifest)

    checks = [
        _rights_check(source_releases),
        _reproducibility_check(package_dir, manifest, acquisition),
        _quality_check(source_quality, candidate_quality, validation),
        _compatibility_check(compatibility, selection),
        _backend_baseline_check(selection, backend_head),
        _nutrient_check(package_dir),
        _curation_check(package_dir, selection),
        _not_applicable_check("recipes", "recipe evidence is outside this FDC composition candidate"),
        _not_applicable_check("portions", "portion-study evidence is outside this FDC composition candidate"),
        _impact_check(package_dir, manifest, impact),
        _production_safety_check(manifest),
        GateCheck(
            "external_review",
            "review_required",
            {
                "selection_reviewer": selection.get("reviewer"),
                "selection_approval_reference": selection.get("approval_reference"),
            },
            "candidate package approval and rollback target must be recorded by a human owner",
        ),
    ]
    review_actions = [check.detail for check in checks if check.status == "review_required"]
    failed = [check.detail for check in checks if check.status == "failed"]
    status = "failed" if failed else ("review_required" if review_actions else "passed")
    return {
        "report_version": RELEASE_GATE_VERSION,
        "status": status,
        "production_eligible": bool(manifest.get("production_eligible", False)),
        "activation_attempted": False,
        "package_manifest_sha256": _sha256((package_dir / "manifest.json").read_bytes()),
        "checks": [check.to_dict() for check in checks],
        "required_human_actions": review_actions,
    }


def _rights_check(source_releases: dict[str, Any]) -> GateCheck:
    sources = source_releases.get("sources", [])
    rights = [source.get("rights_state") for source in sources if isinstance(source, dict)]
    passed = bool(rights) and all(value == "approved" for value in rights)
    return GateCheck(
        "rights",
        "passed" if passed else "failed",
        {"rights_states": rights},
        "all contributing sources have approved rights" if passed else "candidate includes a source without approved rights",
    )


def _reproducibility_check(
    package_dir: Path,
    manifest: dict[str, Any],
    acquisition: dict[str, Any],
) -> GateCheck:
    checksum_file = package_dir / "checksums.sha256"
    mismatches: list[str] = []
    entries = 0
    for line in checksum_file.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        expected, name = line.split("  ", 1)
        entries += 1
        actual = _sha256((package_dir / name).read_bytes())
        if actual != expected:
            mismatches.append(name)
    source_hash = manifest.get("source_releases", [{}])[0].get("artifact_sha256")
    acquired_hash = acquisition.get("extracted_artifact", {}).get("sha256")
    passed = not mismatches and entries > 0 and source_hash == acquired_hash
    return GateCheck(
        "reproducibility",
        "passed" if passed else "failed",
        {"checksum_entries": entries, "checksum_mismatches": mismatches, "artifact_hash_match": source_hash == acquired_hash},
        "package checksums and source artifact hash match" if passed else "package or source artifact checksum evidence does not match",
    )


def _quality_check(
    source_quality: dict[str, Any],
    candidate_quality: dict[str, Any],
    validation: dict[str, Any],
) -> GateCheck:
    candidate_passed = bool(candidate_quality.get("passed")) and bool(validation.get("passed"))
    source_errors = len(source_quality.get("errors", []))
    if not candidate_passed:
        status = "failed"
        detail = "reviewed candidate quality or package validation failed"
    elif source_errors:
        status = "review_required"
        detail = f"review full-source anomalies before any broader selection; {source_errors} hard errors remain outside the reviewed selection"
    else:
        status = "passed"
        detail = "candidate selection quality and package validation passed"
    return GateCheck(
        "quality",
        status,
        {
            "candidate_quality_passed": bool(candidate_quality.get("passed")),
            "candidate_error_count": len(candidate_quality.get("errors", [])),
            "source_quality_passed": bool(source_quality.get("passed")),
            "source_error_count": source_errors,
        },
        detail,
    )


def _compatibility_check(report: dict[str, Any], selection: dict[str, Any]) -> GateCheck:
    passed = report.get("status") == "matched" and report.get("observed_selection_sha256") == selection.get("selection_sha256")
    return GateCheck(
        "backend_compatibility",
        "passed" if passed else "failed",
        {
            "status": report.get("status"),
            "expected_hash": selection.get("selection_sha256"),
            "observed_hash": report.get("observed_selection_sha256"),
        },
        "exact reviewed backend selection reproduced" if passed else "backend selection does not match the approved fingerprint",
    )


def _backend_baseline_check(selection: dict[str, Any], backend_head: str | None) -> GateCheck:
    expected = selection.get("backend_baseline")
    if backend_head is None:
        return GateCheck(
            "backend_baseline",
            "review_required",
            {"expected_head": expected, "observed_head": None},
            "current backend HEAD must be captured before accepting compatibility evidence",
        )
    passed = backend_head == expected
    return GateCheck(
        "backend_baseline",
        "passed" if passed else "review_required",
        {"expected_head": expected, "observed_head": backend_head},
        "compatibility evidence is bound to the current backend baseline" if passed else "backend HEAD differs from the handoff baseline; refresh compatibility evidence or explicitly approve the baseline change",
    )


def _nutrient_check(package_dir: Path) -> GateCheck:
    records = _read_jsonl(package_dir / "normalized-records.jsonl")
    invalid = [record.get("source_id") for record in records if not record.get("attributes", {}).get("canonical_nutrients")]
    passed = not invalid and bool(records)
    return GateCheck(
        "nutrient_semantics",
        "passed" if passed else "failed",
        {"record_count": len(records), "records_without_canonical_nutrients": invalid},
        "all candidate records have explicit canonical nutrient mappings" if passed else "candidate contains a record without canonical nutrient mappings",
    )


def _curation_check(package_dir: Path, selection: dict[str, Any]) -> GateCheck:
    proposals = _read_jsonl(package_dir / "curation-queue.jsonl")
    return GateCheck(
        "identity_curation",
        "review_required",
        {
            "proposal_count": len(proposals),
            "selection_reviewer": selection.get("reviewer"),
            "selection_approval_reference": selection.get("approval_reference"),
        },
        "human reviewer must accept/reject the curation proposals; machine proposals are not activation decisions",
    )


def _not_applicable_check(gate_id: str, detail: str) -> GateCheck:
    return GateCheck(gate_id, "not_applicable", {}, detail)


def _impact_check(package_dir: Path, manifest: dict[str, Any], impact: dict[str, Any]) -> GateCheck:
    expected = manifest.get("impact_report_sha256")
    actual = _sha256((package_dir / "impact-report.json").read_bytes())
    fixed_meals = impact.get("fixed_meal_impact", [])
    passed = expected == actual and isinstance(impact.get("changes"), dict)
    status = "passed" if passed and fixed_meals else "review_required" if passed else "failed"
    detail = "impact report hash matches manifest" if status == "passed" else "fixed-meal impact evidence must be supplied before approval" if passed else "impact report hash or structure does not match manifest"
    return GateCheck(
        "impact",
        status,
        {"manifest_hash": expected, "observed_hash": actual, "fixed_meal_case_count": len(fixed_meals)},
        detail,
    )


def _production_safety_check(manifest: dict[str, Any]) -> GateCheck:
    passed = manifest.get("production_eligible") is False
    return GateCheck(
        "production_safety",
        "passed" if passed else "failed",
        {"production_eligible": manifest.get("production_eligible")},
        "production eligibility is explicitly false" if passed else "candidate package must not be production eligible by default",
    )


def _read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"expected JSON object: {path}")
    return value


def _read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        value = json.loads(line)
        if not isinstance(value, dict):
            raise ValueError(f"expected JSON object line: {path}")
        rows.append(value)
    return rows


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()
