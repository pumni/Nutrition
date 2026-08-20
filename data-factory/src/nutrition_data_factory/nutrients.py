from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any


NUTRIENT_POLICY_VERSION = "fdc-nutrient-crosswalk-0.2.0"
FORBIDDEN_FDC_ENERGY_ID = 1008
SOURCE_PRESERVED_MAPPING_STATUS = "source_preserved_unmapped"


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
    canonical_unit: str
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
            "canonical_unit": self.canonical_unit,
            "policy_version": self.policy_version,
        }


@dataclass(frozen=True)
class NutrientCanonicalization:
    values: tuple[CanonicalNutrientValue, ...]
    rejected: tuple[dict[str, Any], ...]

    @property
    def valid(self) -> bool:
        return not any(
            item["reason_code"]
            in {
                "duplicate_mapping",
                "invalid_value",
                "invalid_unit",
                "status_value_conflict",
                "forbidden_energy",
                "negative_value",
            }
            for item in self.rejected
        )


FDC_CORE_CROSSWALK: tuple[NutrientMapping, ...] = (
    NutrientMapping(1003, "Protein", "G", "declared_or_analytical", "protein_g", "identity", "approved"),
    NutrientMapping(1004, "Total lipid (fat)", "G", "declared_or_analytical", "fat_g", "identity", "approved"),
    NutrientMapping(1005, "Carbohydrate, by difference", "G", "declared_or_analytical", "carbohydrate_g", "identity", "approved"),
    NutrientMapping(2048, "Energy (Atwater Specific)", "KCAL", "atwater_specific", "energy_kcal", "identity", "approved"),
    NutrientMapping(2047, "Energy (Atwater General)", "KCAL", "atwater_general", "energy_kcal", "identity", "approved"),
    NutrientMapping(1002, "Nitrogen", "G", "declared_or_analytical", "nitrogen_g", "identity", "approved"),
    NutrientMapping(1007, "Ash", "G", "declared_or_analytical", "ash_g", "identity", "approved"),
    NutrientMapping(1009, "Starch", "G", "declared_or_analytical", "starch_g", "identity", "approved"),
    NutrientMapping(1010, "Sucrose", "G", "declared_or_analytical", "sucrose_g", "identity", "approved"),
    NutrientMapping(1011, "Glucose", "G", "declared_or_analytical", "glucose_g", "identity", "approved"),
    NutrientMapping(1012, "Fructose", "G", "declared_or_analytical", "fructose_g", "identity", "approved"),
    NutrientMapping(1013, "Lactose", "G", "declared_or_analytical", "lactose_g", "identity", "approved"),
    NutrientMapping(1014, "Maltose", "G", "declared_or_analytical", "maltose_g", "identity", "approved"),
    NutrientMapping(1051, "Water", "G", "declared_or_analytical", "water_g", "identity", "approved"),
    NutrientMapping(1063, "Sugars, Total", "G", "declared_or_analytical", "sugars_total_g", "identity", "approved"),
    NutrientMapping(1071, "Resistant starch", "G", "declared_or_analytical", "resistant_starch_g", "identity", "approved"),
    NutrientMapping(1075, "Galactose", "G", "declared_or_analytical", "galactose_g", "identity", "approved"),
    NutrientMapping(1076, "Raffinose", "G", "declared_or_analytical", "raffinose_g", "identity", "approved"),
    NutrientMapping(1077, "Stachyose", "G", "declared_or_analytical", "stachyose_g", "identity", "approved"),
    NutrientMapping(1079, "Fiber, total dietary", "G", "declared_or_analytical", "fiber_total_g", "identity", "approved"),
    NutrientMapping(1082, "Fiber, soluble", "G", "declared_or_analytical", "fiber_soluble_g", "identity", "approved"),
    NutrientMapping(1084, "Fiber, insoluble", "G", "declared_or_analytical", "fiber_insoluble_g", "identity", "approved"),
    NutrientMapping(1085, "Total fat (NLEA)", "G", "declared_or_analytical", "fat_nlea_g", "identity", "approved"),
    NutrientMapping(1087, "Calcium, Ca", "MG", "declared_or_analytical", "calcium_mg", "identity", "approved"),
    NutrientMapping(1089, "Iron, Fe", "MG", "declared_or_analytical", "iron_mg", "identity", "approved"),
    NutrientMapping(1090, "Magnesium, Mg", "MG", "declared_or_analytical", "magnesium_mg", "identity", "approved"),
    NutrientMapping(1091, "Phosphorus, P", "MG", "declared_or_analytical", "phosphorus_mg", "identity", "approved"),
    NutrientMapping(1092, "Potassium, K", "MG", "declared_or_analytical", "potassium_mg", "identity", "approved"),
    NutrientMapping(1093, "Sodium, Na", "MG", "declared_or_analytical", "sodium_mg", "identity", "approved"),
    NutrientMapping(1095, "Zinc, Zn", "MG", "declared_or_analytical", "zinc_mg", "identity", "approved"),
    NutrientMapping(1098, "Copper, Cu", "MG", "declared_or_analytical", "copper_mg", "identity", "approved"),
    NutrientMapping(1101, "Manganese, Mn", "MG", "declared_or_analytical", "manganese_mg", "identity", "approved"),
    NutrientMapping(1103, "Selenium, Se", "UG", "declared_or_analytical", "selenium_ug", "identity", "approved"),
    NutrientMapping(1105, "Retinol", "UG", "declared_or_analytical", "retinol_ug", "identity", "approved"),
    NutrientMapping(1106, "Vitamin A, RAE", "UG", "declared_or_analytical", "vitamin_a_rae_ug", "identity", "approved"),
    NutrientMapping(1109, "Vitamin E (alpha-tocopherol)", "MG", "declared_or_analytical", "vitamin_e_mg", "identity", "approved"),
    NutrientMapping(1111, "Vitamin D2 (ergocalciferol)", "UG", "declared_or_analytical", "vitamin_d2_ug", "identity", "approved"),
    NutrientMapping(1112, "Vitamin D3 (cholecalciferol)", "UG", "declared_or_analytical", "vitamin_d3_ug", "identity", "approved"),
    NutrientMapping(1114, "Vitamin D (D2 + D3)", "UG", "declared_or_analytical", "vitamin_d_total_ug", "identity", "approved"),
    NutrientMapping(1162, "Vitamin C, total ascorbic acid", "MG", "declared_or_analytical", "vitamin_c_mg", "identity", "approved"),
    NutrientMapping(1165, "Thiamin", "MG", "declared_or_analytical", "thiamin_mg", "identity", "approved"),
    NutrientMapping(1166, "Riboflavin", "MG", "declared_or_analytical", "riboflavin_mg", "identity", "approved"),
    NutrientMapping(1167, "Niacin", "MG", "declared_or_analytical", "niacin_mg", "identity", "approved"),
    NutrientMapping(1170, "Pantothenic acid", "MG", "declared_or_analytical", "pantothenic_acid_mg", "identity", "approved"),
    NutrientMapping(1175, "Vitamin B-6", "MG", "declared_or_analytical", "vitamin_b6_mg", "identity", "approved"),
    NutrientMapping(1176, "Biotin", "UG", "declared_or_analytical", "biotin_ug", "identity", "approved"),
    NutrientMapping(1177, "Folate, total", "UG", "declared_or_analytical", "folate_ug", "identity", "approved"),
    NutrientMapping(1178, "Vitamin B-12", "UG", "declared_or_analytical", "vitamin_b12_ug", "identity", "approved"),
    NutrientMapping(1180, "Choline, total", "MG", "declared_or_analytical", "choline_mg", "identity", "approved"),
    NutrientMapping(1185, "Vitamin K (phylloquinone)", "UG", "declared_or_analytical", "vitamin_k1_ug", "identity", "approved"),
    NutrientMapping(1210, "Tryptophan", "G", "declared_or_analytical", "tryptophan_g", "identity", "approved"),
    NutrientMapping(1211, "Threonine", "G", "declared_or_analytical", "threonine_g", "identity", "approved"),
    NutrientMapping(1212, "Isoleucine", "G", "declared_or_analytical", "isoleucine_g", "identity", "approved"),
    NutrientMapping(1213, "Leucine", "G", "declared_or_analytical", "leucine_g", "identity", "approved"),
    NutrientMapping(1214, "Lysine", "G", "declared_or_analytical", "lysine_g", "identity", "approved"),
    NutrientMapping(1215, "Methionine", "G", "declared_or_analytical", "methionine_g", "identity", "approved"),
    NutrientMapping(1217, "Phenylalanine", "G", "declared_or_analytical", "phenylalanine_g", "identity", "approved"),
    NutrientMapping(1218, "Tyrosine", "G", "declared_or_analytical", "tyrosine_g", "identity", "approved"),
    NutrientMapping(1219, "Valine", "G", "declared_or_analytical", "valine_g", "identity", "approved"),
    NutrientMapping(1220, "Arginine", "G", "declared_or_analytical", "arginine_g", "identity", "approved"),
    NutrientMapping(1221, "Histidine", "G", "declared_or_analytical", "histidine_g", "identity", "approved"),
    NutrientMapping(1222, "Alanine", "G", "declared_or_analytical", "alanine_g", "identity", "approved"),
    NutrientMapping(1223, "Aspartic acid", "G", "declared_or_analytical", "aspartic_acid_g", "identity", "approved"),
    NutrientMapping(1224, "Glutamic acid", "G", "declared_or_analytical", "glutamic_acid_g", "identity", "approved"),
    NutrientMapping(1225, "Glycine", "G", "declared_or_analytical", "glycine_g", "identity", "approved"),
    NutrientMapping(1226, "Proline", "G", "declared_or_analytical", "proline_g", "identity", "approved"),
    NutrientMapping(1227, "Serine", "G", "declared_or_analytical", "serine_g", "identity", "approved"),
    NutrientMapping(1253, "Cholesterol", "MG", "declared_or_analytical", "cholesterol_mg", "identity", "approved"),
    NutrientMapping(1257, "Fatty acids, total trans", "G", "declared_or_analytical", "fatty_acids_trans_g", "identity", "approved"),
    NutrientMapping(1258, "Fatty acids, total saturated", "G", "declared_or_analytical", "fatty_acids_saturated_g", "identity", "approved"),
    NutrientMapping(1292, "Fatty acids, total monounsaturated", "G", "declared_or_analytical", "fatty_acids_monounsaturated_g", "identity", "approved"),
    NutrientMapping(1293, "Fatty acids, total polyunsaturated", "G", "declared_or_analytical", "fatty_acids_polyunsaturated_g", "identity", "approved"),
    NutrientMapping(2033, "Total dietary fiber (AOAC 2011.25)", "G", "declared_or_analytical", "fiber_total_aoac_g", "identity", "approved"),
    NutrientMapping(2038, "High Molecular Weight Dietary Fiber (HMWDF)", "G", "declared_or_analytical", "fiber_high_molecular_weight_g", "identity", "approved"),
    NutrientMapping(2058, "Beta-glucan", "G", "declared_or_analytical", "beta_glucan_g", "identity", "approved"),
    NutrientMapping(2065, "Low Molecular Weight Dietary Fiber (LMWDF)", "G", "declared_or_analytical", "fiber_low_molecular_weight_g", "identity", "approved"),
)

_CROSSWALK_BY_ID = {mapping.source_nutrient_id: mapping for mapping in FDC_CORE_CROSSWALK}


def crosswalk_to_dict() -> dict[str, Any]:
    return {
        "policy_version": NUTRIENT_POLICY_VERSION,
        "mappings": [mapping.to_dict() for mapping in FDC_CORE_CROSSWALK],
        "forbidden_source_ids": [FORBIDDEN_FDC_ENERGY_ID],
        "unmapped_policy": SOURCE_PRESERVED_MAPPING_STATUS,
    }


def build_fdc_nutrient_registry(
    records: list[dict[str, Any]] | tuple[dict[str, Any], ...],
) -> list[dict[str, Any]]:
    """Build a source-derived registry without inventing semantic mappings."""

    observed: dict[int, dict[str, Any]] = {}
    for record in records:
        for item in record.get("nutrients", []):
            nutrient = item.get("nutrient") if isinstance(item, dict) else None
            if not isinstance(nutrient, dict) or not isinstance(nutrient.get("id"), int):
                continue
            source_id = nutrient["id"]
            entry = observed.setdefault(
                source_id,
                {
                    "source_nutrient_id": source_id,
                    "source_label": str(nutrient.get("name", "")),
                    "source_units": set(),
                    "source_methods": set(),
                    "occurrence_count": 0,
                },
            )
            entry["occurrence_count"] += 1
            entry["source_units"].add(str(nutrient.get("unitName", "")))
            mapping = _CROSSWALK_BY_ID.get(source_id)
            entry["source_methods"].add(_source_method(item, mapping) if mapping else _observed_source_method(item))

    output: list[dict[str, Any]] = []
    for source_id in sorted(observed):
        entry = observed[source_id]
        mapping = _CROSSWALK_BY_ID.get(source_id)
        output.append(
            {
                "source_nutrient_id": source_id,
                "source_label": entry["source_label"],
                "source_units": sorted(entry["source_units"]),
                "source_methods": sorted(entry["source_methods"]),
                "occurrence_count": entry["occurrence_count"],
                "target_code": mapping.target_code if mapping else None,
                "canonical_unit": _canonical_unit(mapping) if mapping else None,
                "mapping_status": "approved" if mapping else SOURCE_PRESERVED_MAPPING_STATUS,
                "policy_version": NUTRIENT_POLICY_VERSION,
            }
        )
    return output


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
        unit = str(nutrient.get("unitName", ""))
        if _unit_key(unit) != _unit_key(mapping.source_unit):
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
            if value < 0:
                rejected.append({"source_nutrient_id": source_id, "reason_code": "negative_value"})
                continue
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
                canonical_unit=_canonical_unit(mapping) or "",
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


def _observed_source_method(item: dict[str, Any]) -> str:
    derivation = item.get("foodNutrientDerivation") if isinstance(item, dict) else None
    if isinstance(derivation, dict):
        code = derivation.get("code") or derivation.get("description")
        if isinstance(code, str) and code.strip():
            return code
    return "source_declared_or_unspecified"


def _canonical_unit(mapping: NutrientMapping | None) -> str | None:
    if mapping is None:
        return None
    return _unit_key(mapping.source_unit)


def _unit_key(value: str) -> str:
    normalized = value.strip().casefold().replace("µ", "u").replace("μ", "u")
    return {"kilocalorie": "kcal", "kilocalories": "kcal"}.get(normalized, normalized)
