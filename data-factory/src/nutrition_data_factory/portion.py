from __future__ import annotations

import hashlib
import json
import math
import re
from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal, InvalidOperation
from typing import Any


PORTION_POLICY_VERSION = "portion-study-0.2.0"
PORTION_PROTOCOL_VERSION = "portion-measurement-0.1.0"
MANIFEST_SCHEMA_VERSION = "portion-study-manifest-0.2.0"
MEASUREMENTS_SCHEMA_VERSION = "portion-measurements-0.1.0"
ESTIMATOR_VERSION = "mean_of_sample_means-0.1.0"
BOUND_POLICY_VERSION = "minmax_of_sample_means-0.1.0"
SAMPLE_KINDS = frozenset({"independent_sample", "repeat_weighing"})
REVIEW_PACKET_VERSION = "portion-review-packet-0.1.0"
_MACHINE_REVIEWER = re.compile(r"(?:^|[^a-z0-9])(ai|agent|llm|model|bot)(?:$|[^a-z0-9])", re.IGNORECASE)

_MANIFEST_KEYS = frozenset(
    {
        "schema_version",
        "study_id",
        "target",
        "study_plan",
        "protocol_version",
        "policy_version",
        "estimator_version",
        "bound_policy_version",
        "instrument",
        "tare",
        "operator_ids",
    }
)
_OBSERVATION_KEYS = frozenset(
    {
        "observation_id",
        "sample_id",
        "sample_kind",
        "repeat_of_observation_id",
        "batch_id",
        "mass_g",
        "target",
        "instrument_id",
        "tare_applied",
        "tare_mass_g",
        "calibration_reference",
        "operator_id",
        "measured_at",
    }
)
_OBSERVATION_TARGET_KEYS = frozenset(
    {"food_concept_id", "preparation", "canonical_measure", "represented_quantity", "physical_context"}
)


@dataclass(frozen=True)
class PortionStudyManifest:
    value: dict[str, Any]

    @property
    def target(self) -> dict[str, Any]:
        return self.value["target"]

    @property
    def independent_sample_minimum(self) -> int:
        return self.value["study_plan"]["minimum_independent_samples"]

    @property
    def batch_minimum(self) -> int:
        return self.value["study_plan"]["minimum_batches"]

    def to_dict(self) -> dict[str, Any]:
        return json.loads(json.dumps(self.value, ensure_ascii=False))


@dataclass(frozen=True)
class PortionCompilation:
    manifest: PortionStudyManifest | None
    observations_hash: str | None
    central_mass_g: float | None
    lower_mass_g: float | None
    upper_mass_g: float | None
    sample_count: int
    batch_count: int
    repeat_weighing_count: int
    reviewer: str | None
    reviewer_approval_ref: str | None
    review_ready: bool
    publishable: bool
    publication_blockers: tuple[str, ...]
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
            "batch_count": self.batch_count,
            "repeat_weighing_count": self.repeat_weighing_count,
            "reviewer": self.reviewer,
            "reviewer_approval_ref": self.reviewer_approval_ref,
            "review_ready": self.review_ready,
            "publishable": self.publishable,
            "publication_blockers": list(self.publication_blockers),
            "errors": list(self.errors),
        }


def validate_manifest(value: Any) -> tuple[PortionStudyManifest | None, tuple[dict[str, Any], ...]]:
    errors: list[dict[str, Any]] = []
    if not isinstance(value, dict):
        return None, ({"reason_code": "manifest_not_object"},)
    _check_keys(value, _MANIFEST_KEYS, "manifest", errors)
    for key in sorted(_MANIFEST_KEYS - value.keys()):
        errors.append({"reason_code": "missing_manifest_field", "field": key})
    _require_constant(value, "schema_version", MANIFEST_SCHEMA_VERSION, errors)
    _require_text(value, "study_id", "manifest", errors)
    _require_constant(value, "protocol_version", PORTION_PROTOCOL_VERSION, errors)
    _require_constant(value, "policy_version", PORTION_POLICY_VERSION, errors)
    _require_constant(value, "estimator_version", ESTIMATOR_VERSION, errors)
    _require_constant(value, "bound_policy_version", BOUND_POLICY_VERSION, errors)

    target = value.get("target")
    if not isinstance(target, dict):
        errors.append({"reason_code": "target_not_object"})
    else:
        _check_keys(
            target,
            {"identity", "preparation", "measure"},
            "target",
            errors,
        )
        for key in ("identity", "preparation", "measure"):
            if key not in target:
                errors.append({"reason_code": "missing_target_field", "field": key})
        _validate_decision(
            target.get("identity"),
            path="target.identity",
            value_field="food_concept_id",
            errors=errors,
        )
        _validate_decision(
            target.get("preparation"),
            path="target.preparation",
            value_field="definition",
            errors=errors,
        )
        measure = target.get("measure")
        if not isinstance(measure, dict):
            errors.append({"reason_code": "measure_not_object"})
        else:
            _check_keys(
                measure,
                {
                    "canonical_measure",
                    "original_unit_phrase",
                    "represented_quantity",
                    "context_status",
                    "physical_context",
                    "context_reviewer",
                    "context_review_ref",
                },
                "target.measure",
                errors,
            )
            for key in (
                "canonical_measure",
                "original_unit_phrase",
                "represented_quantity",
                "context_status",
                "physical_context",
                "context_reviewer",
                "context_review_ref",
            ):
                if key not in measure:
                    errors.append({"reason_code": "missing_measure_field", "field": key})
            _require_text(measure, "canonical_measure", "target.measure", errors)
            _require_text(measure, "original_unit_phrase", "target.measure", errors)
            _require_positive_number(measure, "represented_quantity", "target.measure", errors)
            _validate_review_state(
                measure,
                status_field="context_status",
                value_field="physical_context",
                reviewer_field="context_reviewer",
                reference_field="context_review_ref",
                path="target.measure",
                errors=errors,
            )

    plan = value.get("study_plan")
    if not isinstance(plan, dict):
        errors.append({"reason_code": "study_plan_not_object"})
    else:
        _check_keys(
            plan,
            {"minimum_independent_samples", "minimum_batches", "reviewer", "review_ref"},
            "study_plan",
            errors,
        )
        for key in ("minimum_independent_samples", "minimum_batches", "reviewer", "review_ref"):
            if key not in plan:
                errors.append({"reason_code": "missing_study_plan_field", "field": key})
        _require_positive_integer(plan, "minimum_independent_samples", "study_plan", errors)
        _require_positive_integer(plan, "minimum_batches", "study_plan", errors)
        _require_human_review(plan.get("reviewer"), plan.get("review_ref"), "study_plan", errors)

    instrument = value.get("instrument")
    if not isinstance(instrument, dict):
        errors.append({"reason_code": "instrument_not_object"})
    else:
        _check_keys(instrument, {"instrument_id", "resolution_g", "calibration"}, "instrument", errors)
        for key in ("instrument_id", "resolution_g", "calibration"):
            if key not in instrument:
                errors.append({"reason_code": "missing_instrument_field", "field": key})
        _require_text(instrument, "instrument_id", "instrument", errors)
        _require_positive_number(instrument, "resolution_g", "instrument", errors)
        calibration = instrument.get("calibration")
        if not isinstance(calibration, dict):
            errors.append({"reason_code": "calibration_not_object"})
        else:
            calibration_keys = {
                "reference",
                "checked_at",
                "standard_mass_g",
                "observed_mass_g",
                "tolerance_g",
                "operator_id",
            }
            _check_keys(calibration, calibration_keys, "instrument.calibration", errors)
            for key in sorted(calibration_keys - calibration.keys()):
                errors.append({"reason_code": "missing_calibration_field", "field": key})
            for key in ("reference", "operator_id"):
                _require_text(calibration, key, "instrument.calibration", errors)
            _require_timestamp(calibration.get("checked_at"), "instrument.calibration.checked_at", errors)
            _require_positive_number(calibration, "standard_mass_g", "instrument.calibration", errors)
            _require_positive_number(calibration, "observed_mass_g", "instrument.calibration", errors)
            _require_nonnegative_number(calibration, "tolerance_g", "instrument.calibration", errors)
            if all(_is_number(calibration.get(key)) for key in ("standard_mass_g", "observed_mass_g", "tolerance_g")):
                if abs(float(calibration["observed_mass_g"]) - float(calibration["standard_mass_g"])) > float(calibration["tolerance_g"]):
                    errors.append({"reason_code": "calibration_check_out_of_tolerance"})

    tare = value.get("tare")
    if not isinstance(tare, dict):
        errors.append({"reason_code": "tare_not_object"})
    else:
        _check_keys(tare, {"method", "mass_g"}, "tare", errors)
        for key in ("method", "mass_g"):
            if key not in tare:
                errors.append({"reason_code": "missing_tare_field", "field": key})
        _require_text(tare, "method", "tare", errors)
        _require_nonnegative_number(tare, "mass_g", "tare", errors)

    operator_ids = value.get("operator_ids")
    if (
        not isinstance(operator_ids, list)
        or not operator_ids
        or not all(isinstance(item, str) and item.strip() for item in operator_ids)
        or len(set(operator_ids)) != len(operator_ids)
    ):
        errors.append({"reason_code": "invalid_operator_ids"})
    elif isinstance(instrument, dict) and isinstance(instrument.get("calibration"), dict):
        if instrument["calibration"].get("operator_id") not in operator_ids:
            errors.append({"reason_code": "calibration_operator_not_in_manifest"})

    if errors:
        return None, tuple(errors)
    return PortionStudyManifest(value=value), ()


def compile_portion_study(
    manifest_value: Any,
    observations: Any,
    *,
    reviewer_approval_ref: str | None = None,
    reviewer: str | None = None,
) -> PortionCompilation:
    manifest, manifest_errors = validate_manifest(manifest_value)
    errors = list(manifest_errors)
    document_errors: list[dict[str, Any]] = []
    if isinstance(observations, dict):
        parsed_observations = _read_measurement_document(observations, document_errors)
    elif isinstance(observations, list):
        parsed_observations = observations
    else:
        parsed_observations = []
        document_errors.append({"reason_code": "observations_not_array"})
    errors.extend(document_errors)

    try:
        observations_hash = hashlib.sha256(
            json.dumps(
                parsed_observations,
                ensure_ascii=False,
                sort_keys=True,
                separators=(",", ":"),
                allow_nan=False,
            ).encode("utf-8")
        ).hexdigest()
    except (TypeError, ValueError):
        observations_hash = None
        errors.append({"reason_code": "observations_not_json"})

    if not parsed_observations:
        errors.append({"reason_code": "observations_required"})

    primary_observations: dict[str, dict[str, Any]] = {}
    primary_by_sample: dict[str, dict[str, Any]] = {}
    repeat_rows: list[dict[str, Any]] = []
    sample_masses: dict[str, list[Decimal]] = {}
    sample_batches: dict[str, str] = {}
    seen_observation_ids: set[str] = set()
    repeat_count = 0
    for index, observation in enumerate(parsed_observations):
        path = f"observations[{index}]"
        if not isinstance(observation, dict):
            errors.append({"observation_index": index, "reason_code": "observation_not_object"})
            continue
        _check_keys(observation, _OBSERVATION_KEYS, path, errors, index)
        for key in sorted(_OBSERVATION_KEYS - observation.keys()):
            errors.append({"observation_index": index, "reason_code": "missing_observation_field", "field": key})
        observation_id = observation.get("observation_id")
        sample_id = observation.get("sample_id")
        batch_id = observation.get("batch_id")
        sample_kind = observation.get("sample_kind")
        repeat_ref = observation.get("repeat_of_observation_id")
        for field, field_value in (("observation_id", observation_id), ("sample_id", sample_id), ("batch_id", batch_id)):
            if not isinstance(field_value, str) or not field_value.strip():
                errors.append({"observation_index": index, "reason_code": "invalid_observation_field", "field": field})
        if not isinstance(sample_kind, str) or sample_kind not in SAMPLE_KINDS:
            errors.append({"observation_index": index, "reason_code": "invalid_sample_kind"})
        if isinstance(observation_id, str):
            if observation_id in seen_observation_ids:
                errors.append({"observation_index": index, "reason_code": "duplicate_observation_id"})
            seen_observation_ids.add(observation_id)
        if not _is_positive_number(observation.get("mass_g")):
            errors.append({"observation_index": index, "reason_code": "mass_g_must_be_positive_finite"})
        if observation.get("tare_applied") is not True:
            errors.append({"observation_index": index, "reason_code": "tare_not_confirmed"})
        if manifest is not None:
            instrument = manifest.value["instrument"]
            tare = manifest.value["tare"]
            if observation.get("instrument_id") != instrument["instrument_id"]:
                errors.append({"observation_index": index, "reason_code": "instrument_mismatch"})
            if observation.get("calibration_reference") != instrument["calibration"]["reference"]:
                errors.append({"observation_index": index, "reason_code": "calibration_mismatch"})
            if observation.get("operator_id") not in manifest.value["operator_ids"]:
                errors.append({"observation_index": index, "reason_code": "operator_not_in_manifest"})
            if not _equal_number(observation.get("tare_mass_g"), tare["mass_g"]):
                errors.append({"observation_index": index, "reason_code": "tare_mass_mismatch"})
            if observation.get("target") != _measurement_target(manifest.target):
                errors.append({"observation_index": index, "reason_code": "target_context_mismatch"})
        _require_timestamp(
            observation.get("measured_at"),
            f"{path}.measured_at",
            errors,
            observation_index=index,
        )
        if sample_kind == "independent_sample":
            if repeat_ref is not None:
                errors.append({"observation_index": index, "reason_code": "independent_sample_has_repeat_parent"})
            if isinstance(sample_id, str):
                if sample_id in primary_by_sample:
                    errors.append({"observation_index": index, "reason_code": "duplicate_independent_sample_id"})
                else:
                    primary_by_sample[sample_id] = observation
                    if isinstance(observation_id, str):
                        primary_observations[observation_id] = observation
                    if isinstance(batch_id, str):
                        sample_batches[sample_id] = batch_id
                    if _is_positive_number(observation.get("mass_g")):
                        sample_masses.setdefault(sample_id, []).append(_decimal(observation["mass_g"]))
        elif sample_kind == "repeat_weighing":
            repeat_count += 1
            if not isinstance(repeat_ref, str) or not repeat_ref.strip():
                errors.append({"observation_index": index, "reason_code": "repeat_parent_required"})
            else:
                repeat_rows.append(observation)
            if isinstance(sample_id, str) and _is_positive_number(observation.get("mass_g")):
                sample_masses.setdefault(sample_id, []).append(_decimal(observation["mass_g"]))

    for observation in repeat_rows:
        parent_id = observation.get("repeat_of_observation_id")
        parent = primary_observations.get(parent_id) if isinstance(parent_id, str) else None
        if parent is None:
            errors.append({"reason_code": "repeat_parent_not_independent_sample"})
            continue
        if observation.get("sample_id") != parent.get("sample_id"):
            errors.append({"reason_code": "repeat_sample_id_mismatch"})
        if observation.get("batch_id") != parent.get("batch_id"):
            errors.append({"reason_code": "repeat_batch_id_mismatch"})

    if manifest is not None:
        independent_count = len(primary_by_sample)
        batch_count = len(set(sample_batches.values()))
        if independent_count < manifest.independent_sample_minimum:
            errors.append({"reason_code": "insufficient_independent_samples"})
        if batch_count < manifest.batch_minimum:
            errors.append({"reason_code": "insufficient_independent_batches"})
    else:
        independent_count = len(primary_by_sample)
        batch_count = len(set(sample_batches.values()))

    if reviewer_approval_ref is not None or reviewer is not None:
        _require_human_review(reviewer, reviewer_approval_ref, "measurement_review", errors)

    blockers: set[str] = {"publication_disabled_tooling_only"}
    if not parsed_observations:
        blockers.add("measurement_data_missing")
    if manifest is not None:
        identity = manifest.target["identity"]
        preparation = manifest.target["preparation"]
        measure = manifest.target["measure"]
        if identity["status"] != "reviewed":
            blockers.add("food_identity_unresolved")
        if preparation["status"] != "reviewed":
            blockers.add("preparation_state_unresolved")
        if measure["context_status"] != "reviewed":
            blockers.add("physical_measure_context_unresolved")
    else:
        blockers.add("manifest_invalid")
    if reviewer_approval_ref is None or reviewer is None:
        blockers.add("measurement_review_missing")
    if errors:
        blockers.add("validation_failed")

    target_reviewed = (
        manifest is not None
        and manifest.target["identity"]["status"] == "reviewed"
        and manifest.target["preparation"]["status"] == "reviewed"
        and manifest.target["measure"]["context_status"] == "reviewed"
    )
    has_review = reviewer_approval_ref is not None and reviewer is not None
    can_calculate = not errors and target_reviewed and bool(parsed_observations)
    central: float | None = None
    lower: float | None = None
    upper: float | None = None
    if can_calculate:
        sample_means = [sum(values, Decimal(0)) / Decimal(len(values)) for values in sample_masses.values()]
        if sample_means:
            central = float(sum(sample_means, Decimal(0)) / Decimal(len(sample_means)))
            lower = float(min(sample_means))
            upper = float(max(sample_means))
            if not (0 < lower <= central <= upper):
                errors.append({"reason_code": "mass_bounds_invariant_failed"})
                blockers.add("validation_failed")
                central = lower = upper = None

    review_ready = can_calculate and has_review and not errors
    return PortionCompilation(
        manifest=manifest,
        observations_hash=observations_hash,
        central_mass_g=central,
        lower_mass_g=lower,
        upper_mass_g=upper,
        sample_count=independent_count,
        batch_count=batch_count,
        repeat_weighing_count=repeat_count,
        reviewer=reviewer,
        reviewer_approval_ref=reviewer_approval_ref,
        review_ready=review_ready,
        publishable=False,
        publication_blockers=tuple(sorted(blockers)),
        errors=tuple(errors),
    )


def build_portion_review_packet(
    compilation: PortionCompilation,
    *,
    manifest_sha256: str | None = None,
    measurements_sha256: str | None = None,
    manifest_artifact: dict[str, Any] | None = None,
    measurements_artifact: dict[str, Any] | None = None,
    compiler_sha256: str | None = None,
) -> dict[str, Any]:
    manifest = compilation.manifest
    target = manifest.target if manifest else None
    return {
        "schema_version": REVIEW_PACKET_VERSION,
        "policy_version": PORTION_POLICY_VERSION,
        "compiler_sha256": compiler_sha256,
        "status": "review_ready_publication_blocked" if compilation.review_ready else "blocked",
        "study_id": manifest.value["study_id"] if manifest else None,
        "study_parameters": (
            {
                "study_plan": manifest.value["study_plan"],
                "protocol_version": manifest.value["protocol_version"],
                "policy_version": manifest.value["policy_version"],
                "estimator_version": manifest.value["estimator_version"],
                "bound_policy_version": manifest.value["bound_policy_version"],
                "instrument": manifest.value["instrument"],
                "tare": manifest.value["tare"],
                "operator_ids": manifest.value["operator_ids"],
            }
            if manifest
            else None
        ),
        "input_artifacts": {
            "manifest": manifest_artifact or _hash_reference(manifest_sha256),
            "measurements": measurements_artifact or _hash_reference(measurements_sha256),
            "canonical_observations_sha256": compilation.observations_hash,
        },
        "target_review": _target_review_summary(target, compilation.errors),
        "measurement_review": {
            "reviewer": compilation.reviewer,
            "review_ref": compilation.reviewer_approval_ref,
            "review_ready": compilation.review_ready,
        },
        "measurement_summary": {
            "independent_sample_count": compilation.sample_count,
            "independent_batch_count": compilation.batch_count,
            "repeat_weighing_count": compilation.repeat_weighing_count,
            "central_mass_g": compilation.central_mass_g,
            "lower_mass_g": compilation.lower_mass_g,
            "upper_mass_g": compilation.upper_mass_g,
            "review_ready": compilation.review_ready,
            "validation_passed": compilation.passed,
        },
        "validation_errors": list(compilation.errors),
        "publication": {
            "status": "blocked",
            "publishable": False,
            "blockers": list(compilation.publication_blockers),
            "release_created": False,
            "activation_attempted": False,
        },
    }


def _read_measurement_document(value: dict[str, Any], errors: list[dict[str, Any]]) -> list[Any]:
    allowed = {"schema_version", "observations"}
    _check_keys(value, allowed, "measurements", errors)
    for key in sorted(allowed - value.keys()):
        errors.append({"reason_code": "missing_measurement_document_field", "field": key})
    if value.get("schema_version") != MEASUREMENTS_SCHEMA_VERSION:
        errors.append({"reason_code": "unsupported_measurements_schema"})
    observations = value.get("observations")
    if not isinstance(observations, list):
        errors.append({"reason_code": "observations_not_array"})
        return []
    return observations


def _validate_decision(
    value: Any,
    *,
    path: str,
    value_field: str,
    errors: list[dict[str, Any]],
) -> None:
    if not isinstance(value, dict):
        errors.append({"reason_code": "decision_not_object", "field": path})
        return
    keys = {"status", value_field, "reviewer", "review_ref"}
    _check_keys(value, keys, path, errors)
    for key in sorted(keys - value.keys()):
        errors.append({"reason_code": "missing_decision_field", "field": f"{path}.{key}"})
    _validate_review_state(
        value,
        status_field="status",
        value_field=value_field,
        reviewer_field="reviewer",
        reference_field="review_ref",
        path=path,
        errors=errors,
    )


def _validate_review_state(
    value: dict[str, Any],
    *,
    status_field: str,
    value_field: str,
    reviewer_field: str,
    reference_field: str,
    path: str,
    errors: list[dict[str, Any]],
) -> None:
    status = value.get(status_field)
    if not isinstance(status, str) or status not in {"unresolved", "reviewed"}:
        errors.append({"reason_code": "invalid_review_status", "field": f"{path}.{status_field}"})
        return
    fields = (value_field, reviewer_field, reference_field)
    if status == "unresolved":
        for field in fields:
            if value.get(field) is not None:
                errors.append({"reason_code": "unresolved_decision_has_value", "field": f"{path}.{field}"})
        return
    for field in fields:
        if not isinstance(value.get(field), str) or not value[field].strip():
            errors.append({"reason_code": "reviewed_decision_field_required", "field": f"{path}.{field}"})
    reviewer = value.get(reviewer_field)
    if isinstance(reviewer, str) and _MACHINE_REVIEWER.search(reviewer):
        errors.append({"reason_code": "machine_reviewer_forbidden", "field": f"{path}.{reviewer_field}"})


def _measurement_target(target: dict[str, Any]) -> dict[str, Any]:
    identity = target["identity"]
    preparation = target["preparation"]
    measure = target["measure"]
    return {
        "food_concept_id": identity["food_concept_id"],
        "preparation": preparation["definition"],
        "canonical_measure": measure["canonical_measure"],
        "represented_quantity": measure["represented_quantity"],
        "physical_context": measure["physical_context"],
    }


def _target_review_summary(
    target: dict[str, Any] | None,
    errors: tuple[dict[str, Any], ...],
) -> dict[str, Any] | None:
    if target is None:
        return None
    return {
        "identity": target["identity"],
        "preparation": target["preparation"],
        "measure": target["measure"],
        "target_context_matches_measurements": not any(
            error.get("reason_code") == "target_context_mismatch" for error in errors
        ),
    }


def _hash_reference(value: str | None) -> dict[str, str] | None:
    return {"sha256": value} if value is not None else None


def _require_constant(value: dict[str, Any], field: str, expected: str, errors: list[dict[str, Any]]) -> None:
    if value.get(field) != expected:
        errors.append({"reason_code": "unsupported_version", "field": field})


def _require_text(value: dict[str, Any], field: str, path: str, errors: list[dict[str, Any]]) -> None:
    if not isinstance(value.get(field), str) or not value[field].strip():
        errors.append({"reason_code": "invalid_text_field", "field": f"{path}.{field}"})


def _require_positive_integer(value: dict[str, Any], field: str, path: str, errors: list[dict[str, Any]]) -> None:
    item = value.get(field)
    if isinstance(item, bool) or not isinstance(item, int) or item < 1:
        errors.append({"reason_code": "positive_integer_required", "field": f"{path}.{field}"})


def _require_positive_number(value: dict[str, Any], field: str, path: str, errors: list[dict[str, Any]]) -> None:
    if not _is_positive_number(value.get(field)):
        errors.append({"reason_code": "positive_finite_number_required", "field": f"{path}.{field}"})


def _require_nonnegative_number(value: dict[str, Any], field: str, path: str, errors: list[dict[str, Any]]) -> None:
    if not _is_number(value.get(field)) or float(value[field]) < 0:
        errors.append({"reason_code": "nonnegative_finite_number_required", "field": f"{path}.{field}"})


def _require_timestamp(value: Any, path: str, errors: list[dict[str, Any]], observation_index: int | None = None) -> None:
    context: dict[str, Any] = {"reason_code": "timezone_aware_timestamp_required", "field": path}
    if observation_index is not None:
        context["observation_index"] = observation_index
    if not isinstance(value, str):
        errors.append(context)
        return
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        errors.append(context)
        return
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        errors.append(context)


def _require_human_review(reviewer: Any, review_ref: Any, path: str, errors: list[dict[str, Any]]) -> None:
    if not isinstance(reviewer, str) or not reviewer.strip():
        errors.append({"reason_code": "reviewer_required", "field": f"{path}.reviewer"})
    elif _MACHINE_REVIEWER.search(reviewer):
        errors.append({"reason_code": "machine_reviewer_forbidden", "field": f"{path}.reviewer"})
    if not isinstance(review_ref, str) or not review_ref.strip():
        errors.append({"reason_code": "review_reference_required", "field": f"{path}.review_ref"})


def _check_keys(
    value: dict[str, Any],
    allowed: set[str] | frozenset[str],
    path: str,
    errors: list[dict[str, Any]],
    observation_index: int | None = None,
) -> None:
    for key in sorted(value.keys() - allowed):
        error: dict[str, Any] = {"reason_code": "unknown_field", "field": f"{path}.{key}"}
        if observation_index is not None:
            error["observation_index"] = observation_index
        errors.append(error)


def _is_number(value: Any) -> bool:
    return not isinstance(value, bool) and isinstance(value, (int, float)) and math.isfinite(float(value))


def _is_positive_number(value: Any) -> bool:
    return _is_number(value) and float(value) > 0


def _equal_number(left: Any, right: Any) -> bool:
    return _is_number(left) and _is_number(right) and Decimal(str(left)) == Decimal(str(right))


def _decimal(value: int | float) -> Decimal:
    try:
        return Decimal(str(value))
    except InvalidOperation as exc:
        raise ValueError("mass must be a finite decimal") from exc
