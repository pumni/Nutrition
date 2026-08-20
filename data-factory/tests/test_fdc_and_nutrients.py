from __future__ import annotations

import hashlib
import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.adapters.fdc_foundation import (  # noqa: E402
    FDC_FOUNDATION_RELEASE,
    FdcFoundationAdapter,
)
from nutrition_data_factory.nutrients import (  # noqa: E402
    build_fdc_nutrient_registry,
    canonicalize_fdc_nutrients,
)


class FdcAndNutrientTests(unittest.TestCase):
    def test_adapter_pins_release_preserves_payload_and_reports_every_bad_row(self) -> None:
        fixture = ROOT / "tests" / "fixtures" / "fdc-foundation-april-2026.synthetic.json"
        payload = fixture.read_bytes()
        result = FdcFoundationAdapter().parse(
            payload,
            release=FDC_FOUNDATION_RELEASE,
            expected_sha256=hashlib.sha256(payload).hexdigest(),
        )
        self.assertEqual(result.raw_record_count, 3)
        self.assertEqual(len(result.accepted_records), 1)
        self.assertEqual(len(result.rejected_records), 2)
        self.assertEqual(
            [reject.reason_code for reject in result.rejected_records],
            ["null_record", "invalid_fdc_id"],
        )
        record = result.accepted_records[0]
        self.assertEqual(record.fdc_id, 990000001)
        self.assertEqual(record.characteristics[0]["code"], "fixture_only")
        self.assertEqual(record.portions[0]["modifier"], "fixture-only")
        self.assertEqual(record.nutrients[0]["nutrient"]["id"], 1003)
        self.assertEqual(len(record.payload_sha256), 64)
        self.assertEqual(result.schema_fingerprint, FdcFoundationAdapter().parse(payload).schema_fingerprint)

    def test_adapter_fails_closed_on_checksum_or_release_mismatch(self) -> None:
        payload = b'{"FoundationFoods": []}'
        with self.assertRaises(ValueError):
            FdcFoundationAdapter().parse(payload, expected_sha256="0" * 64)
        with self.assertRaises(ValueError):
            FdcFoundationAdapter().parse(payload, release="2021-2023")

    def test_crosswalk_uses_explicit_ids_and_prefers_specific_energy(self) -> None:
        nutrients = [
            {"amount": 1, "nutrient": {"id": 1003, "name": "wrong display name", "unitName": "G"}},
            {"amount": 2, "nutrient": {"id": 1004, "name": "wrong display name", "unitName": "G"}},
            {"amount": 3, "nutrient": {"id": 1005, "name": "wrong display name", "unitName": "G"}},
            {"amount": 40, "nutrient": {"id": 2047, "unitName": "KCAL"}},
            {"amount": 41, "nutrient": {"id": 2048, "unitName": "KCAL"}},
            {"amount": 42, "nutrient": {"id": 1008, "unitName": "KCAL"}},
        ]
        result = canonicalize_fdc_nutrients(nutrients)
        self.assertFalse(result.valid)
        self.assertEqual(
            [value.source_nutrient_id for value in result.values],
            [1003, 1004, 1005, 2048],
        )
        self.assertTrue(any(item["reason_code"] == "forbidden_energy" for item in result.rejected))
        self.assertTrue(any(item["reason_code"] == "energy_fallback_not_selected" for item in result.rejected))
        self.assertEqual(result.values[-1].source_method, "atwater_specific")

    def test_value_states_remain_distinct_and_energy_is_not_derived(self) -> None:
        result = canonicalize_fdc_nutrients(
            [
                {"amount": None, "value_status": "trace", "nutrient": {"id": 1003, "unitName": "G"}},
                {"amount": None, "value_status": "not_detected", "nutrient": {"id": 1004, "unitName": "G"}},
                {"amount": 0, "nutrient": {"id": 1005, "unitName": "G"}},
                {"amount": None, "nutrient": {"id": 2048, "unitName": "KCAL"}},
            ]
        )
        self.assertEqual(
            {value.target_code: value.value_status for value in result.values},
            {
                "protein_g": "trace",
                "fat_g": "not_detected",
                "carbohydrate_g": "zero",
                "energy_kcal": "missing",
            },
        )
        self.assertNotIn("energy_kcal", {value.target_code for value in canonicalize_fdc_nutrients([]).values})

    def test_source_registry_preserves_unmapped_ids_and_negative_values_are_quarantinable(self) -> None:
        registry = build_fdc_nutrient_registry(
            [
                {"nutrients": [{"amount": 1, "nutrient": {"id": 1003, "name": "Protein", "unitName": "g"}}]},
                {"nutrients": [{"amount": 2, "nutrient": {"id": 9999, "name": "Source-only nutrient", "unitName": "mg"}}]},
            ]
        )
        by_id = {item["source_nutrient_id"]: item for item in registry}
        self.assertEqual(by_id[1003]["mapping_status"], "approved")
        self.assertEqual(by_id[9999]["mapping_status"], "source_preserved_unmapped")
        result = canonicalize_fdc_nutrients([
            {"amount": -0.1, "nutrient": {"id": 1005, "name": "Carbohydrate, by difference", "unitName": "G"}}
        ])
        self.assertFalse(result.values)
        self.assertIn("negative_value", {item["reason_code"] for item in result.rejected})

    def test_extended_crosswalk_covers_broad_nutrient_groups_with_source_units(self) -> None:
        result = canonicalize_fdc_nutrients(
            [
                {"amount": 41, "nutrient": {"id": 1087, "name": "Calcium, Ca", "unitName": "mg"}},
                {"amount": 16.2, "nutrient": {"id": 1103, "name": "Selenium, Se", "unitName": "µg"}},
                {"amount": 420, "nutrient": {"id": 1253, "name": "Cholesterol", "unitName": "mg"}},
            ]
        )
        self.assertEqual({value.target_code for value in result.values}, {"calcium_mg", "selenium_ug", "cholesterol_mg"})
        self.assertEqual({value.canonical_unit for value in result.values}, {"mg", "ug"})
        self.assertTrue(result.valid)


if __name__ == "__main__":
    unittest.main()
