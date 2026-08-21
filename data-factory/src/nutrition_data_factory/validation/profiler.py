from __future__ import annotations

import hashlib
import math
from dataclasses import dataclass
from typing import Any

from ..models import SourceMetadata
from ..provenance import canonical_json_bytes
from ..source_registry import production_rights_error


QUALITY_RULE_VERSION = "quality-profiler-0.1.0"


@dataclass(frozen=True)
class QualityProfileReport:
    rule_version: str
    source_code: str
    release: str
    input_sha256: str
    errors: tuple[dict[str, Any], ...]
    warnings: tuple[dict[str, Any], ...]
    statistics: dict[str, int]

    @property
    def passed(self) -> bool:
        return not self.errors

    def to_dict(self) -> dict[str, Any]:
        return {
            "rule_version": self.rule_version,
            "source_code": self.source_code,
            "release": self.release,
            "input_sha256": self.input_sha256,
            "passed": self.passed,
            "errors": list(self.errors),
            "warnings": list(self.warnings),
            "statistics": self.statistics,
        }


def profile_records(
    records: list[dict[str, Any]],
    *,
    metadata: SourceMetadata,
    artifact_sha256: str,
    expected_artifact_sha256: str | None = None,
    production_candidate: bool = False,
    previous_values: dict[str, float] | None = None,
) -> QualityProfileReport:
    """Profile evidence without changing, repairing, averaging, or imputing any record."""

    input_sha256 = hashlib.sha256(canonical_json_bytes(records)).hexdigest()
    errors: list[dict[str, Any]] = []
    warnings: list[dict[str, Any]] = []
    if expected_artifact_sha256 is not None and artifact_sha256 != expected_artifact_sha256.lower():
        errors.append({"reason_code": "artifact_checksum_mismatch"})
    if len(artifact_sha256) != 64:
        errors.append({"reason_code": "invalid_artifact_sha256"})
    if production_candidate:
        rights_error = production_rights_error(metadata)
        if rights_error is not None:
            errors.append({"reason_code": "rights_gate", "detail": rights_error})
    seen_ids: set[str] = set()
    numeric_value_count = 0
    missing_basis_count = 0
    for index, record in enumerate(records):
        if not isinstance(record, dict):
            errors.append({"record_index": index, "reason_code": "record_not_object"})
            continue
        source_id = record.get("source_id", record.get("fdc_id"))
        if source_id is None:
            errors.append({"record_index": index, "reason_code": "missing_source_record_id"})
        else:
            source_id = str(source_id)
            if source_id in seen_ids:
                errors.append({"record_index": index, "reason_code": "duplicate_source_record_id", "source_id": source_id})
            seen_ids.add(source_id)
        if record.get("source_code", metadata.code) != metadata.code:
            errors.append({"record_index": index, "reason_code": "source_code_mismatch"})
        if record.get("release", metadata.release) != metadata.release:
            errors.append({"record_index": index, "reason_code": "release_mismatch"})
        payload_sha256 = record.get("payload_sha256")
        if not isinstance(payload_sha256, str) or len(payload_sha256) != 64:
            errors.append({"record_index": index, "reason_code": "missing_payload_hash"})
        nutrients = record.get("nutrients", record.get("foodNutrients", []))
        if not isinstance(nutrients, list):
            errors.append({"record_index": index, "reason_code": "nutrients_not_array"})
            continue
        for nutrient_index, nutrient in enumerate(nutrients):
            _profile_nutrient(
                nutrient,
                index,
                nutrient_index,
                errors,
                warnings,
                previous_values,
            )
            if isinstance(nutrient, dict) and isinstance(nutrient.get("amount"), (int, float)):
                numeric_value_count += 1
            if isinstance(nutrient, dict) and not nutrient.get("basis") and not record.get("basis"):
                missing_basis_count += 1
        portions = record.get("portions", record.get("foodPortions", []))
        if not isinstance(portions, list):
            errors.append({"record_index": index, "reason_code": "portions_not_array"})
        else:
            for portion_index, portion in enumerate(portions):
                if not isinstance(portion, dict):
                    errors.append({"record_index": index, "reason_code": "portion_not_object", "portion_index": portion_index})
                elif portion.get("source_record_id") not in (None, str(source_id)):
                    errors.append({"record_index": index, "reason_code": "portion_reference_mismatch", "portion_index": portion_index})
    if missing_basis_count:
        warnings.append({"reason_code": "missing_basis", "count": missing_basis_count})
    statistics = {
        "input_record_count": len(records),
        "unique_source_record_count": len(seen_ids),
        "numeric_value_count": numeric_value_count,
        "error_count": len(errors),
        "warning_count": len(warnings),
    }
    return QualityProfileReport(
        rule_version=QUALITY_RULE_VERSION,
        source_code=metadata.code,
        release=metadata.release,
        input_sha256=input_sha256,
        errors=tuple(errors),
        warnings=tuple(warnings),
        statistics=statistics,
    )


def _profile_nutrient(
    nutrient: Any,
    record_index: int,
    nutrient_index: int,
    errors: list[dict[str, Any]],
    warnings: list[dict[str, Any]],
    previous_values: dict[str, float] | None,
) -> None:
    if not isinstance(nutrient, dict):
        errors.append({"record_index": record_index, "nutrient_index": nutrient_index, "reason_code": "nutrient_not_object"})
        return
    source_nutrient = nutrient.get("nutrient")
    if not isinstance(source_nutrient, dict) or not isinstance(source_nutrient.get("id"), int):
        errors.append({"record_index": record_index, "nutrient_index": nutrient_index, "reason_code": "missing_source_nutrient_id"})
    if not isinstance(source_nutrient, dict) or not isinstance(source_nutrient.get("unitName"), str) or not source_nutrient.get("unitName"):
        errors.append({"record_index": record_index, "nutrient_index": nutrient_index, "reason_code": "missing_unit"})
    amount = nutrient.get("amount")
    if amount is None:
        return
    if isinstance(amount, bool) or not isinstance(amount, (int, float)) or not math.isfinite(float(amount)):
        errors.append({"record_index": record_index, "nutrient_index": nutrient_index, "reason_code": "invalid_numeric_value"})
        return
    if amount < 0:
        errors.append({"record_index": record_index, "nutrient_index": nutrient_index, "reason_code": "negative_numeric_value"})
    if amount > 1_000_000:
        warnings.append({"record_index": record_index, "nutrient_index": nutrient_index, "reason_code": "extreme_numeric_value"})
    if previous_values and isinstance(source_nutrient, dict):
        code = str(source_nutrient.get("id"))
        previous = previous_values.get(code)
        if previous not in (None, 0) and abs(float(amount) - previous) / abs(previous) > 0.5:
            warnings.append({"record_index": record_index, "nutrient_index": nutrient_index, "reason_code": "large_prior_release_delta"})
