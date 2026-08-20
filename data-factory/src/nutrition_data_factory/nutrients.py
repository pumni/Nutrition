from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any


NUTRIENT_POLICY_VERSION = "fdc-nutrient-crosswalk-0.1.0"
FORBIDDEN_FDC_ENERGY_ID = 1008


@dataclass(frozen=True)
class NutrientMapping:
    source_nutrient_id: int
    source_label: str
    source_unit: str
    source_method: str
    target_code: str
    conversion: str
    status: str
    policy_version: str = NUTRIENT_POLICY_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_nutrient_id": self.source_nutrient_id,
            "source_label": self.source_label,
            "source_unit": self.source_unit,
            "source_method": self.source_method,
            "target_code": self.target_code,
            "conversion": self.conversion,
            "status": self.status,
            "policy_version": self.policy_version,
        }


@dataclass(frozen=True)
class CanonicalNutrientValue:
    target_code: str
    source_nutrient_id: int
    source_label: str
    source_unit: str
    source_method: str
    value: float | None
    value_status: str
    conversion: str
    policy_version: str = NUTRIENT_POLICY_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "target_code": self.target_code,
            "source_nutrient_id": self.source_nutrient_id,
            "source_label": self.source_label,
            "source_unit": self.source_unit,
            "source_method": self.source_method,
            "value": self.value,
            "value_status": self.value_status,
            "conversion": self.conversion,
            "policy_version": self.policy_version,
        }


@dataclass(frozen=True)
class NutrientCanonicalization:
    values: tuple[CanonicalNutrientValue, ...]
    rejected: tuple[dict[str, Any], ...]

    @property
    def valid(self) -> bool:
        return not any(item["reason_code"] in {"duplicate_mapping", "invalid_value", "forbidden_energy"} for item in self.rejected)


FDC_CORE_CROSSWALK: tuple[NutrientMapping, ...] = (
    NutrientMapping(1003, "Protein", "G", "declared_or_analytical", "protein_g", "identity", "approved"),
    NutrientMapping(1004, "Total lipid (fat)", "G", "declared_or_analytical", "fat_g", "identity", "approved"),
    NutrientMapping(1005, "Carbohydrate, by difference", "G", "declared_or_analytical", "carbohydrate_g", "identity", "approved"),
    NutrientMapping(2048, "Energy (Atwater Specific)", "KCAL", "atwater_specific", "energy_kcal", "identity", "approved"),
    NutrientMapping(2047, "Energy (Atwater General)", "KCAL", "atwater_general", "energy_kcal", "identity", "approved"),
)

_CROSSWALK_BY_ID = {mapping.source_nutrient_id: mapping for mapping in FDC_CORE_CROSSWALK}


def crosswalk_to_dict() -> dict[str, Any]:
    return {
        "policy_version": NUTRIENT_POLICY_VERSION,
        "mappings": [mapping.to_dict() for mapping in FDC_CORE_CROSSWALK],
        "forbidden_source_ids": [FORBIDDEN_FDC_ENERGY_ID],
    }


def canonicalize_fdc_nutrients(nutrients: list[dict[str, Any]] | tuple[dict[str, Any], ...]) -> NutrientCanonicalization:
    candidates: dict[int, dict[str, Any]] = {}
    rejected: list[dict[str, Any]] = []
    for index, item in enumerate(nutrients):
        nutrient = item.get("nutrient") if isinstance(item, dict) else None
        if not isinstance(nutrient, dict) or not isinstance(nutrient.get("id"), int):
            rejected.append({"index": index, "reason_code": "invalid_nutrient_id"})
            continue
        source_id = nutrient["id"]
        if source_id == FORBIDDEN_FDC_ENERGY_ID:
            rejected.append({"index": index, "source_nutrient_id": source_id, "reason_code": "forbidden_energy"})
            continue
        if source_id not in _CROSSWALK_BY_ID:
            rejected.append({"index": index, "source_nutrient_id": source_id, "reason_code": "unmapped_source_nutrient"})
            continue
        if source_id in candidates:
            rejected.append({"index": index, "source_nutrient_id": source_id, "reason_code": "duplicate_mapping"})
            continue
        candidates[source_id] = item

    values: list[CanonicalNutrientValue] = []
    for source_id in sorted(candidates):
        item = candidates[source_id]
        mapping = _CROSSWALK_BY_ID[source_id]
        nutrient = item["nutrient"]
        unit = str(nutrient.get("unitName", "")).upper()
        if unit != mapping.source_unit:
            rejected.append({"source_nutrient_id": source_id, "reason_code": "invalid_unit", "expected": mapping.source_unit, "actual": unit})
            continue
        method = _source_method(item, mapping)
        amount = item.get("amount")
        status = str(item.get("value_status", item.get("status", "numeric")))
        if amount is None:
            if status not in {"missing", "trace", "not_detected"}:
                status = "missing"
            value: float | None = None
        elif isinstance(amount, bool) or not isinstance(amount, (int, float)) or not math.isfinite(float(amount)):
            rejected.append({"source_nutrient_id": source_id, "reason_code": "invalid_value"})
            continue
        else:
            value = float(amount)
            if status in {"missing", "trace", "not_detected"} and status != "numeric":
                rejected.append({"source_nutrient_id": source_id, "reason_code": "status_value_conflict"})
                continue
            status = "zero" if value == 0 else "numeric"
        values.append(
            CanonicalNutrientValue(
                target_code=mapping.target_code,
                source_nutrient_id=source_id,
                source_label=str(nutrient.get("name", mapping.source_label)),
                source_unit=unit,
                source_method=method,
                value=value,
                value_status=status,
                conversion=mapping.conversion,
            )
        )
    # A specific Atwater value is selected over general; the general candidate remains provenance-visible in rejects.
    energy_ids = [value.source_nutrient_id for value in values if value.target_code == "energy_kcal"]
    if 2048 in energy_ids and 2047 in energy_ids:
        values = [value for value in values if value.source_nutrient_id != 2047]
        rejected.append({"source_nutrient_id": 2047, "reason_code": "energy_fallback_not_selected", "selected": 2048})
    elif 2048 not in energy_ids and 2047 in energy_ids:
        pass
    return NutrientCanonicalization(tuple(values), tuple(rejected))


def _source_method(item: dict[str, Any], mapping: NutrientMapping) -> str:
    derivation = item.get("foodNutrientDerivation")
    if isinstance(derivation, dict):
        code = derivation.get("code") or derivation.get("description")
        if isinstance(code, str) and code.strip():
            return code
    return mapping.source_method

