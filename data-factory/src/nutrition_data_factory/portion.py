from __future__ import annotations

import hashlib
import json
import math
from dataclasses import dataclass
from statistics import mean
from typing import Any


PORTION_POLICY_VERSION = "portion-study-0.1.0"
ALLOWED_SAMPLE_KINDS = frozenset({"independent_sample", "repeat_weighing"})
FORBIDDEN_REVIEWER_MARKERS = ("ai", "agent", "llm", "model", "bot")


@dataclass(frozen=True)
class PortionStudyManifest:
    study_id: str
    food_concept_id: str
    preparation: str
    measure: str
    physical_context: str
    protocol_version: str
    estimator_version: str
    instrument_id: str
    tare_method: str
    calibration_reference: str
    operator_ids: tuple[str, ...]
    policy_version: str = PORTION_POLICY_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "study_id": self.study_id,
            "food_concept_id": self.food_concept_id,
            "preparation": self.preparation,
            "measure": self.measure,
            "physical_context": self.physical_context,
            "protocol_version": self.protocol_version,
            "estimator_version": self.estimator_version,
            "instrument_id": self.instrument_id,
            "tare_method": self.tare_method,
            "calibration_reference": self.calibration_reference,
            "operator_ids": list(self.operator_ids),
            "policy_version": self.policy_version,
        }


@dataclass(frozen=True)
class PortionCompilation:
    manifest: PortionStudyManifest | None
    observations_hash: str | None
    central_mass_g: float | None
    lower_mass_g: float | None
    upper_mass_g: float | None
    sample_count: int
    repeat_weighing_count: int
    publishable: bool
    reviewer_approval_ref: str | None
    errors: tuple[dict[str, Any], ...]

    @property
    def passed(self) -> bool:
        return not self.errors

    def to_dict(self) -> dict[str, Any]:
        return {
            "manifest": self.manifest.to_dict() if self.manifest else None,
            "observations_hash": self.observations_hash,
            "central_mass_g": self.central_mass_g,
            "lower_mass_g": self.lower_mass_g,
            "upper_mass_g": self.upper_mass_g,
            "sample_count": self.sample_count,
            "repeat_weighing_count": self.repeat_weighing_count,
            "publishable": self.publishable,
            "reviewer_approval_ref": self.reviewer_approval_ref,
            "errors": list(self.errors),
        }


def validate_manifest(value: dict[str, Any]) -> tuple[PortionStudyManifest | None, tuple[dict[str, Any], ...]]:
    required = (
        "study_id",
        "food_concept_id",
        "preparation",
        "measure",
        "physical_context",
        "protocol_version",
        "estimator_version",
        "instrument_id",
        "tare_method",
        "calibration_reference",
        "operator_ids",
    )
    errors: list[dict[str, Any]] = []
    for key in required:
        if key not in value:
            errors.append({"reason_code": "missing_manifest_field", "field": key})
    if errors:
        return None, tuple(errors)
    for key in required[:-1]:
        if not isinstance(value[key], str) or not value[key].strip():
            errors.append({"reason_code": "invalid_manifest_field", "field": key})
    operator_ids = value["operator_ids"]
    if not isinstance(operator_ids, list) or not operator_ids or not all(isinstance(item, str) and item.strip() for item in operator_ids):
        errors.append({"reason_code": "invalid_operator_ids"})
    if errors:
        return None, tuple(errors)
    return (
        PortionStudyManifest(
            study_id=value["study_id"],
            food_concept_id=value["food_concept_id"],
            preparation=value["preparation"],
            measure=value["measure"],
            physical_context=value["physical_context"],
            protocol_version=value["protocol_version"],
            estimator_version=value["estimator_version"],
            instrument_id=value["instrument_id"],
            tare_method=value["tare_method"],
            calibration_reference=value["calibration_reference"],
            operator_ids=tuple(value["operator_ids"]),
            policy_version=value.get("policy_version", PORTION_POLICY_VERSION),
        ),
        (),
    )


def compile_portion_study(
    manifest_value: dict[str, Any],
    observations: list[dict[str, Any]],
    *,
    reviewer_approval_ref: str | None = None,
    reviewer: str | None = None,
) -> PortionCompilation:
    manifest, manifest_errors = validate_manifest(manifest_value)
    errors = list(manifest_errors)
    if not isinstance(observations, list) or not observations:
        errors.append({"reason_code": "observations_required"})
        observations = []
    sample_masses: dict[str, list[float]] = {}
    repeat_count = 0
    for index, observation in enumerate(observations):
        if not isinstance(observation, dict):
            errors.append({"observation_index": index, "reason_code": "observation_not_object"})
            continue
        sample_kind = observation.get("sample_kind")
        if sample_kind not in ALLOWED_SAMPLE_KINDS:
            errors.append({"observation_index": index, "reason_code": "invalid_sample_kind"})
            continue
        sample_id = observation.get("sample_id")
        if not isinstance(sample_id, str) or not sample_id.strip():
            errors.append({"observation_index": index, "reason_code": "missing_sample_id"})
            continue
        if observation.get("instrument_id") != (manifest.instrument_id if manifest else None):
            errors.append({"observation_index": index, "reason_code": "instrument_mismatch"})
        if observation.get("tare_applied") is not True:
            errors.append({"observation_index": index, "reason_code": "tare_not_confirmed"})
        if observation.get("calibration_reference") != (manifest.calibration_reference if manifest else None):
            errors.append({"observation_index": index, "reason_code": "calibration_mismatch"})
        if observation.get("operator_id") not in (manifest.operator_ids if manifest else ()):
            errors.append({"observation_index": index, "reason_code": "operator_not_in_manifest"})
        mass = observation.get("mass_g")
        if isinstance(mass, bool) or not isinstance(mass, (int, float)) or not math.isfinite(float(mass)) or float(mass) < 0:
            errors.append({"observation_index": index, "reason_code": "mass_g_required_numeric"})
            continue
        if "unit" in observation and "mass_g" not in observation:
            errors.append({"observation_index": index, "reason_code": "unit_word_cannot_create_mass"})
            continue
        sample_masses.setdefault(sample_id, []).append(float(mass))
        if sample_kind == "repeat_weighing":
            repeat_count += 1
    reviewer_error = _reviewer_error(reviewer, reviewer_approval_ref)
    if reviewer_error is not None:
        errors.append(reviewer_error)
    observations_hash = hashlib.sha256(
        json.dumps(observations, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    if errors or manifest is None:
        return PortionCompilation(
            manifest=manifest,
            observations_hash=observations_hash,
            central_mass_g=None,
            lower_mass_g=None,
            upper_mass_g=None,
            sample_count=len(sample_masses),
            repeat_weighing_count=repeat_count,
            publishable=False,
            reviewer_approval_ref=reviewer_approval_ref,
            errors=tuple(errors),
        )
    independent_sample_means = [mean(masses) for masses in sample_masses.values()]
    central = mean(independent_sample_means)
    lower = min(independent_sample_means)
    upper = max(independent_sample_means)
    return PortionCompilation(
        manifest=manifest,
        observations_hash=observations_hash,
        central_mass_g=central,
        lower_mass_g=lower,
        upper_mass_g=upper,
        sample_count=len(independent_sample_means),
        repeat_weighing_count=repeat_count,
        publishable=reviewer_approval_ref is not None,
        reviewer_approval_ref=reviewer_approval_ref,
        errors=(),
    )


def _reviewer_error(reviewer: str | None, approval_ref: str | None) -> dict[str, Any] | None:
    if approval_ref is None:
        return None
    if not isinstance(reviewer, str) or not reviewer.strip():
        return {"reason_code": "reviewer_required_for_approval"}
    lowered = reviewer.casefold()
    if any(marker in lowered for marker in FORBIDDEN_REVIEWER_MARKERS):
        return {"reason_code": "machine_reviewer_forbidden"}
    return None

