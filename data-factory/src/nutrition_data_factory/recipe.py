from __future__ import annotations

import math
from typing import Any


RECIPE_POLICY_VERSION = "recipe-evidence-0.1.0"


def validate_recipe_evidence(
    recipe: dict[str, Any],
    *,
    known_source_food_ids: set[str],
) -> dict[str, Any]:
    errors: list[dict[str, Any]] = []
    recipe_id = recipe.get("recipe_id")
    if not isinstance(recipe_id, str) or not recipe_id.strip():
        errors.append({"reason_code": "missing_recipe_id"})
    components = recipe.get("components")
    if not isinstance(components, list) or not components:
        errors.append({"reason_code": "missing_components"})
        components = []
    for index, component in enumerate(components):
        if not isinstance(component, dict):
            errors.append({"component_index": index, "reason_code": "component_not_object"})
            continue
        source_id = component.get("source_food_id")
        if not isinstance(source_id, str) or source_id not in known_source_food_ids:
            errors.append({"component_index": index, "reason_code": "unresolved_ingredient", "source_food_id": source_id})
        weight = component.get("resolved_weight_g")
        if not _positive_finite(weight):
            errors.append({"component_index": index, "reason_code": "missing_resolved_weight_g"})
    yield_weight = recipe.get("cooked_total_weight_g", recipe.get("raw_total_weight_g"))
    if not _positive_finite(yield_weight):
        errors.append({"reason_code": "missing_positive_yield_weight"})
    if recipe.get("status") == "published" and not recipe.get("reviewer"):
        errors.append({"reason_code": "published_recipe_missing_human_reviewer"})
    return {
        "policy_version": RECIPE_POLICY_VERSION,
        "recipe_id": recipe_id,
        "status": "valid" if not errors else "invalid",
        "errors": errors,
        "component_count": len(components),
        "calculation_allowed": not errors,
    }


def calculate_recipe_composition(
    recipe: dict[str, Any],
    *,
    ingredient_values: dict[str, list[dict[str, Any]]],
) -> dict[str, Any]:
    validation = validate_recipe_evidence(recipe, known_source_food_ids=set(ingredient_values))
    if not validation["calculation_allowed"]:
        raise ValueError(json_error(validation))
    yield_weight = float(recipe.get("cooked_total_weight_g", recipe.get("raw_total_weight_g")))
    totals: dict[str, float] = {}
    trace: list[dict[str, Any]] = []
    for index, component in enumerate(recipe["components"]):
        source_id = component["source_food_id"]
        weight = float(component["resolved_weight_g"])
        for value in ingredient_values[source_id]:
            code = value.get("target_code")
            amount = value.get("value")
            if not isinstance(code, str) or not _finite(amount):
                continue
            basis_g = float(value.get("basis_amount_g", 100.0))
            contribution = float(amount) * weight / basis_g
            totals[code] = totals.get(code, 0.0) + contribution
            trace.append({"component_index": index, "source_food_id": source_id, "target_code": code, "input_value": amount, "resolved_weight_g": weight, "contribution": contribution})
    values = [
        {
            "target_code": code,
            "value": amount * 100.0 / yield_weight,
            "value_status": "calculated",
            "basis_amount_g": 100.0,
            "policy_version": RECIPE_POLICY_VERSION,
        }
        for code, amount in sorted(totals.items())
    ]
    return {
        "recipe_id": recipe["recipe_id"],
        "profile_type": "recipe_calculated",
        "status": "in_review",
        "values": values,
        "calculation_trace": trace,
        "policy_version": RECIPE_POLICY_VERSION,
        "human_review_required": True,
    }


def json_error(value: object) -> str:
    return str(value)


def _positive_finite(value: Any) -> bool:
    return _finite(value) and float(value) > 0


def _finite(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(float(value))
