from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any, Iterable


IMPACT_REPORT_VERSION = "release-impact-0.1.0"
CHANGE_CATEGORIES = ("identities", "names", "mappings", "profiles", "values", "recipes", "portions")


@dataclass(frozen=True)
class ImpactReport:
    report_version: str
    changes: dict[str, dict[str, list[dict[str, Any]]]]
    nutrient_deltas: list[dict[str, Any]]
    coverage: dict[str, Any]
    fixed_meal_impact: list[dict[str, Any]]
    material_delta_flags: list[dict[str, Any]]

    def to_dict(self) -> dict[str, Any]:
        return {
            "report_version": self.report_version,
            "changes": self.changes,
            "nutrient_deltas": self.nutrient_deltas,
            "coverage": self.coverage,
            "fixed_meal_impact": self.fixed_meal_impact,
            "material_delta_flags": self.material_delta_flags,
            "clinical_correctness_claimed": False,
        }


def build_impact_report(
    previous: dict[str, list[dict[str, Any]]],
    current: dict[str, list[dict[str, Any]]],
    *,
    benchmark_cases: list[dict[str, Any]] | None = None,
    fixed_meal_cases: list[dict[str, Any]] | None = None,
    material_relative_threshold: float = 0.2,
) -> ImpactReport:
    changes: dict[str, dict[str, list[dict[str, Any]]]] = {}
    for category in CHANGE_CATEGORIES:
        changes[category] = _diff_category(previous.get(category, []), current.get(category, []))
    nutrient_deltas = _nutrient_deltas(previous.get("values", []), current.get("values", []))
    flags = [
        {
            "value_key": delta["value_key"],
            "relative_delta": delta["relative_delta"],
            "reason_code": "material_nutrient_delta",
        }
        for delta in nutrient_deltas
        if delta.get("relative_delta") is not None and abs(delta["relative_delta"]) > material_relative_threshold
    ]
    coverage = _coverage_report(benchmark_cases or [], current)
    fixed_impact = _fixed_meal_impact(fixed_meal_cases or [])
    return ImpactReport(
        report_version=IMPACT_REPORT_VERSION,
        changes=changes,
        nutrient_deltas=nutrient_deltas,
        coverage=coverage,
        fixed_meal_impact=fixed_impact,
        material_delta_flags=flags,
    )


def _diff_category(previous: list[dict[str, Any]], current: list[dict[str, Any]]) -> dict[str, list[dict[str, Any]]]:
    old = {_entry_key(item): item for item in previous}
    new = {_entry_key(item): item for item in current}
    added = [{"key": key, "new": new[key]} for key in sorted(new.keys() - old.keys())]
    removed = [{"key": key, "old": old[key]} for key in sorted(old.keys() - new.keys())]
    changed = [
        {"key": key, "old": old[key], "new": new[key]}
        for key in sorted(old.keys() & new.keys())
        if old[key] != new[key]
    ]
    return {"added": added, "removed": removed, "changed": changed}


def _entry_key(item: dict[str, Any]) -> str:
    for key in ("id", "identity_id", "name_id", "mapping_id", "profile_id", "value_id", "recipe_id", "portion_id", "source_id"):
        if key in item:
            return f"{key}:{item[key]}"
    return "canonical:" + repr(sorted(item.items()))


def _nutrient_deltas(previous: list[dict[str, Any]], current: list[dict[str, Any]]) -> list[dict[str, Any]]:
    old = {_entry_key(item): item for item in previous}
    new = {_entry_key(item): item for item in current}
    output: list[dict[str, Any]] = []
    for key in sorted(old.keys() & new.keys()):
        old_value = old[key].get("value")
        new_value = new[key].get("value")
        if not _finite_number(old_value) or not _finite_number(new_value):
            continue
        delta = float(new_value) - float(old_value)
        relative = None if float(old_value) == 0 else delta / abs(float(old_value))
        output.append(
            {
                "value_key": key,
                "old_value": old_value,
                "new_value": new_value,
                "delta": delta,
                "relative_delta": relative,
                "old_provenance": old[key].get("provenance"),
                "new_provenance": new[key].get("provenance"),
            }
        )
    return output


def _coverage_report(benchmark_cases: list[dict[str, Any]], current: dict[str, list[dict[str, Any]]]) -> dict[str, Any]:
    available = {
        str(item.get("candidate_id", item.get("id")))
        for category in ("identities", "profiles", "values")
        for item in current.get(category, [])
    }
    by_class: dict[str, dict[str, int]] = {}
    for case in benchmark_cases:
        evidence_class = str(case.get("evidence_class", "unspecified"))
        candidate_id = str(case.get("candidate_id", ""))
        bucket = by_class.setdefault(evidence_class, {"total": 0, "covered": 0})
        bucket["total"] += 1
        if candidate_id in available:
            bucket["covered"] += 1
    return {
        "benchmark_case_count": len(benchmark_cases),
        "by_evidence_class": by_class,
        "clinical_correctness_claimed": False,
    }


def _fixed_meal_impact(cases: list[dict[str, Any]]) -> list[dict[str, Any]]:
    output: list[dict[str, Any]] = []
    for case in cases:
        items = []
        for item in case.get("items", []):
            old_value = item.get("old_value")
            new_value = item.get("new_value")
            delta = None
            if _finite_number(old_value) and _finite_number(new_value):
                delta = float(new_value) - float(old_value)
            items.append(
                {
                    "item_key": item.get("item_key"),
                    "old_value": old_value,
                    "new_value": new_value,
                    "delta": delta,
                    "old_provenance": item.get("old_provenance"),
                    "new_provenance": item.get("new_provenance"),
                }
            )
        output.append({"case_id": case.get("case_id"), "items": items, "clinical_correctness_claimed": False})
    return output


def _finite_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(float(value))

