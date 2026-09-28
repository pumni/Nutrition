from __future__ import annotations

from collections import Counter
from typing import Any, Iterable

from .adapters.fndds_survey import FnddsSourceRecord


FNDDS_NUTRIENT_MAPPING_VERSION = "fndds-required-nutrients-0.1.0"
FNDDS_EXPECTED_NO_PROFILE_FOOD_CODES = frozenset({"11000000"})

# FNDDS 2021-2023 Appendix K crosswalks code 208 to FDC 1008/2047.
# This source-specific mapping does not reuse Foundation's fdc_energy_v1 rule.
_REQUIRED_NUTRIENTS: tuple[dict[str, Any], ...] = (
    {
        "fndds_nutrient_code": "208",
        "source_nutrient_ids": frozenset({1008, 2047}),
        "source_unit": "kcal",
        "target_code": "energy_kcal",
    },
    {
        "fndds_nutrient_code": "203",
        "source_nutrient_ids": frozenset({1003}),
        "source_unit": "g",
        "target_code": "protein_g",
    },
    {
        "fndds_nutrient_code": "205",
        "source_nutrient_ids": frozenset({1005}),
        "source_unit": "g",
        "target_code": "carbohydrate_g",
    },
    {
        "fndds_nutrient_code": "204",
        "source_nutrient_ids": frozenset({1004}),
        "source_unit": "g",
        "target_code": "fat_g",
    },
)


def summarize_fndds_required_nutrients(nutrients: Iterable[dict[str, Any]]) -> dict[str, Any]:
    """Validate the four product-required FNDDS nutrients without exporting values."""

    rows = tuple(nutrients)
    mapped: list[dict[str, Any]] = []
    rejected: list[dict[str, Any]] = []
    for definition in _REQUIRED_NUTRIENTS:
        code = definition["fndds_nutrient_code"]
        matches = [
            item
            for item in rows
            if isinstance(item.get("nutrient"), dict)
            and item["nutrient"].get("number") == code
        ]
        if not matches:
            rejected.append({"reason_code": "missing_required_nutrient", "fndds_nutrient_code": code})
            continue
        if len(matches) != 1:
            rejected.append({"reason_code": "ambiguous_required_nutrient", "fndds_nutrient_code": code})
            continue

        item = matches[0]
        source = item["nutrient"]
        source_id = source.get("id")
        if source_id not in definition["source_nutrient_ids"]:
            rejected.append(
                {
                    "reason_code": "unknown_source_nutrient_id",
                    "fndds_nutrient_code": code,
                    "source_nutrient_id": source_id,
                }
            )
            continue
        unit = source.get("unitName")
        if unit != definition["source_unit"]:
            rejected.append(
                {
                    "reason_code": "unexpected_source_unit",
                    "fndds_nutrient_code": code,
                    "source_nutrient_id": source_id,
                    "source_unit": unit,
                }
            )
            continue
        amount = item.get("amount")
        if amount is None:
            rejected.append(
                {
                    "reason_code": "missing_required_value",
                    "fndds_nutrient_code": code,
                    "source_nutrient_id": source_id,
                }
            )
            continue

        mapped.append(
            {
                "target_code": definition["target_code"],
                "fndds_nutrient_code": code,
                "source_nutrient_id": source_id,
                "source_label": source.get("name"),
                "source_unit": unit,
                "source_method": item.get("foodNutrientDerivation"),
                "source_method_available": "foodNutrientDerivation" in item,
                "value_state": "present",
                "basis": "per_100g_edible_portion",
            }
        )

    mapped.sort(key=lambda item: item["target_code"])
    rejected.sort(key=lambda item: (item["reason_code"], item.get("fndds_nutrient_code", "")))
    return {
        "mapping_version": FNDDS_NUTRIENT_MAPPING_VERSION,
        "complete": not rejected and len(mapped) == len(_REQUIRED_NUTRIENTS),
        "required_nutrients": mapped,
        "rejected": rejected,
    }


def summarize_fndds_source_nutrient_coverage(
    records: Iterable[FnddsSourceRecord],
) -> dict[str, Any]:
    complete_count = 0
    expected_profileless: list[dict[str, str]] = []
    unexpected_incomplete: list[dict[str, Any]] = []
    rejection_counts: Counter[str] = Counter()

    for record in records:
        summary = summarize_fndds_required_nutrients(record.nutrients)
        if summary["complete"]:
            complete_count += 1
            continue
        if record.food_code in FNDDS_EXPECTED_NO_PROFILE_FOOD_CODES and not record.nutrients:
            expected_profileless.append(
                {"fndds_food_code": record.food_code, "description": record.description}
            )
            continue
        reasons = sorted({item["reason_code"] for item in summary["rejected"]})
        for reason in reasons:
            rejection_counts[reason] += 1
        unexpected_incomplete.append(
            {
                "fndds_food_code": record.food_code,
                "fdc_id": record.fdc_id,
                "reason_codes": reasons,
            }
        )

    expected_profileless.sort(key=lambda item: item["fndds_food_code"])
    unexpected_incomplete.sort(key=lambda item: item["fndds_food_code"])
    return {
        "mapping_version": FNDDS_NUTRIENT_MAPPING_VERSION,
        "record_count": complete_count + len(expected_profileless) + len(unexpected_incomplete),
        "complete_required_profiles": complete_count,
        "expected_profileless_records": expected_profileless,
        "unexpected_incomplete_records": unexpected_incomplete,
        "unexpected_rejection_reason_counts": dict(sorted(rejection_counts.items())),
        "passed": not unexpected_incomplete,
    }


def source_nutrient_definitions(nutrients: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    """Retain source nutrient IDs, codes, labels, units, and any supplied method."""

    result = []
    for item in nutrients:
        source = item["nutrient"]
        result.append(
            {
                "fndds_nutrient_code": source["number"],
                "source_nutrient_id": source["id"],
                "source_label": source["name"],
                "source_unit": source["unitName"],
                "source_method": item.get("foodNutrientDerivation"),
                "source_method_available": "foodNutrientDerivation" in item,
            }
        )
    result.sort(key=lambda item: (item["fndds_nutrient_code"], item["source_nutrient_id"]))
    return result
