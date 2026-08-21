from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.compatibility import (  # noqa: E402
    compare_fdc_selection,
    load_compatibility_manifest,
)
from nutrition_data_factory.source_registry import SourceRegistry  # noqa: E402
from nutrition_data_factory.validation.profiler import profile_records  # noqa: E402


class QualityCompatibilityTests(unittest.TestCase):
    def test_quality_profiler_reports_without_mutating_input(self) -> None:
        metadata = SourceRegistry.load(ROOT / "config" / "source_registry.json").get("synthetic_fixture")
        records = [
            {
                "source_code": "synthetic_fixture",
                "release": "0.1.0",
                "source_id": "one",
                "payload_sha256": "a" * 64,
                "nutrients": [
                    {"amount": 2, "nutrient": {"id": 1003, "unitName": "G"}}
                ],
                "portions": [],
            }
        ]
        before = copy.deepcopy(records)
        report = profile_records(
            records,
            metadata=metadata,
            artifact_sha256="b" * 64,
            expected_artifact_sha256="b" * 64,
        )
        self.assertTrue(report.passed)
        self.assertTrue(any(item["reason_code"] == "missing_basis" for item in report.warnings))
        self.assertEqual(records, before)

    def test_quality_profiler_fails_hard_and_flags_soft_anomalies(self) -> None:
        metadata = SourceRegistry.load(ROOT / "config" / "source_registry.json").get("synthetic_fixture")
        records = [
            {
                "source_code": "synthetic_fixture",
                "release": "wrong-release",
                "source_id": "duplicate",
                "payload_sha256": "c" * 64,
                "nutrients": [
                    {"amount": -1, "nutrient": {"id": 1003, "unitName": "G"}},
                    {"amount": 2_000_000, "nutrient": {"id": 1004, "unitName": "G"}},
                ],
                "portions": [],
            },
            {
                "source_code": "synthetic_fixture",
                "release": "0.1.0",
                "source_id": "duplicate",
                "payload_sha256": "d" * 64,
                "nutrients": [],
                "portions": [],
            },
        ]
        report = profile_records(
            records,
            metadata=metadata,
            artifact_sha256="e" * 64,
            previous_values={"1004": 1},
        )
        error_codes = {item["reason_code"] for item in report.errors}
        warning_codes = {item["reason_code"] for item in report.warnings}
        self.assertFalse(report.passed)
        self.assertIn("release_mismatch", error_codes)
        self.assertIn("duplicate_source_record_id", error_codes)
        self.assertIn("negative_numeric_value", error_codes)
        self.assertIn("extreme_numeric_value", warning_codes)
        self.assertIn("large_prior_release_delta", warning_codes)

    def test_backend_compatibility_reads_exact_selection_and_never_adds_records(self) -> None:
        manifest = load_compatibility_manifest(ROOT / "config" / "backend-fdc-selection.json")
        matched = compare_fdc_selection(manifest, manifest["fdc_ids"])
        self.assertTrue(matched.matches)
        mismatch = compare_fdc_selection(manifest, [*manifest["fdc_ids"], 999999999])
        self.assertFalse(mismatch.matches)
        self.assertEqual(mismatch.unexpected_ids, (999999999,))
        self.assertEqual(len(mismatch.expected_ids), 20)


if __name__ == "__main__":
    unittest.main()
