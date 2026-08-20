from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


COMPATIBILITY_VERSION = "backend-fdc-selection-0.1.0"


@dataclass(frozen=True)
class CompatibilityReport:
    expected_selection_sha256: str
    observed_selection_sha256: str
    expected_ids: tuple[int, ...]
    observed_ids: tuple[int, ...]
    missing_ids: tuple[int, ...]
    unexpected_ids: tuple[int, ...]
    status: str
    source_evidence: str

    @property
    def matches(self) -> bool:
        return self.status == "matched"

    def to_dict(self) -> dict[str, Any]:
        return {
            "compatibility_version": COMPATIBILITY_VERSION,
            "expected_selection_sha256": self.expected_selection_sha256,
            "observed_selection_sha256": self.observed_selection_sha256,
            "expected_ids": list(self.expected_ids),
            "observed_ids": list(self.observed_ids),
            "missing_ids": list(self.missing_ids),
            "unexpected_ids": list(self.unexpected_ids),
            "status": self.status,
            "source_evidence": self.source_evidence,
        }


def load_compatibility_manifest(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict) or not isinstance(value.get("fdc_ids"), list):
        raise ValueError("compatibility manifest must contain fdc_ids")
    ids = value["fdc_ids"]
    if not ids or not all(isinstance(item, int) for item in ids):
        raise ValueError("compatibility fdc_ids must be non-empty integers")
    expected = selection_fingerprint(ids)
    if value.get("selection_sha256") != expected:
        raise ValueError("compatibility manifest selection_sha256 does not match its IDs")
    return value


def compare_fdc_selection(manifest: dict[str, Any], observed_ids: Iterable[int]) -> CompatibilityReport:
    expected_ids = tuple(sorted(manifest["fdc_ids"]))
    observed = tuple(sorted(set(observed_ids)))
    expected_set = set(expected_ids)
    observed_set = set(observed)
    missing = tuple(sorted(expected_set - observed_set))
    unexpected = tuple(sorted(observed_set - expected_set))
    observed_hash = selection_fingerprint(observed)
    status = "matched" if not missing and not unexpected and observed_hash == manifest["selection_sha256"] else "mismatch"
    return CompatibilityReport(
        expected_selection_sha256=manifest["selection_sha256"],
        observed_selection_sha256=observed_hash,
        expected_ids=expected_ids,
        observed_ids=observed,
        missing_ids=missing,
        unexpected_ids=unexpected,
        status=status,
        source_evidence=str(manifest.get("source_evidence", "")),
    )


def selection_fingerprint(ids: Iterable[int]) -> str:
    joined = ",".join(str(value) for value in sorted(set(ids)))
    return hashlib.sha256(joined.encode("utf-8")).hexdigest()

