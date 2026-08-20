from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.portion import compile_portion_study, validate_manifest  # noqa: E402


MANIFEST = {
    "study_id": "study-synthetic-001",
    "food_concept_id": "fixture-food-1",
    "preparation": "fixture-preparation",
    "measure": "fixture-vessel",
    "physical_context": "fixture-context",
    "protocol_version": "portion-protocol-0.1.0",
    "estimator_version": "mean_minmax_by_independent_sample-0.1.0",
    "instrument_id": "scale-fixture-1",
    "tare_method": "empty-vessel-tare",
    "calibration_reference": "calibration-fixture-1",
    "operator_ids": ["operator-1"],
}


def observations() -> list[dict[str, object]]:
    return [
        {"observation_id": "w1", "sample_id": "sample-1", "sample_kind": "independent_sample", "mass_g": 100, "instrument_id": "scale-fixture-1", "tare_applied": True, "calibration_reference": "calibration-fixture-1", "operator_id": "operator-1"},
        {"observation_id": "w2", "sample_id": "sample-1", "sample_kind": "repeat_weighing", "mass_g": 102, "instrument_id": "scale-fixture-1", "tare_applied": True, "calibration_reference": "calibration-fixture-1", "operator_id": "operator-1"},
        {"observation_id": "w3", "sample_id": "sample-2", "sample_kind": "independent_sample", "mass_g": 110, "instrument_id": "scale-fixture-1", "tare_applied": True, "calibration_reference": "calibration-fixture-1", "operator_id": "operator-1"},
        {"observation_id": "w4", "sample_id": "sample-2", "sample_kind": "repeat_weighing", "mass_g": 111, "instrument_id": "scale-fixture-1", "tare_applied": True, "calibration_reference": "calibration-fixture-1", "operator_id": "operator-1"},
    ]


class PortionTests(unittest.TestCase):
    def test_manifest_requires_protocol_instrument_tare_calibration_and_operator(self) -> None:
        manifest, errors = validate_manifest(MANIFEST)
        self.assertIsNotNone(manifest)
        self.assertEqual(errors, ())
        invalid = dict(MANIFEST)
        del invalid["calibration_reference"]
        _, errors = validate_manifest(invalid)
        self.assertTrue(any(error["reason_code"] == "missing_manifest_field" for error in errors))

    def test_compiler_distinguishes_independent_samples_and_repeats(self) -> None:
        raw_observations = observations()
        before = copy.deepcopy(raw_observations)
        result = compile_portion_study(MANIFEST, raw_observations)
        self.assertTrue(result.passed)
        self.assertEqual(result.sample_count, 2)
        self.assertEqual(result.repeat_weighing_count, 2)
        self.assertEqual(result.central_mass_g, 105.75)
        self.assertEqual(result.lower_mass_g, 101)
        self.assertEqual(result.upper_mass_g, 110.5)
        self.assertFalse(result.publishable)
        self.assertEqual(raw_observations, before)

    def test_publishable_state_requires_named_human_reviewer(self) -> None:
        approved = compile_portion_study(
            MANIFEST,
            observations(),
            reviewer_approval_ref="review://portion/1",
            reviewer="reviewer@example",
        )
        self.assertTrue(approved.publishable)
        rejected = compile_portion_study(
            MANIFEST,
            observations(),
            reviewer_approval_ref="review://portion/1",
            reviewer="AI-agent",
        )
        self.assertFalse(rejected.passed)
        self.assertFalse(rejected.publishable)

    def test_unit_words_cannot_be_converted_into_mass(self) -> None:
        invalid = observations()
        invalid[0] = {
            "observation_id": "w1",
            "sample_id": "sample-1",
            "sample_kind": "independent_sample",
            "unit": "bát",
            "instrument_id": "scale-fixture-1",
            "tare_applied": True,
            "calibration_reference": "calibration-fixture-1",
            "operator_id": "operator-1",
        }
        result = compile_portion_study(MANIFEST, invalid)
        self.assertFalse(result.passed)
        self.assertTrue(any(error["reason_code"] == "mass_g_required_numeric" for error in result.errors))


if __name__ == "__main__":
    unittest.main()

