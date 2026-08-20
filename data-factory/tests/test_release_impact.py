from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.artifacts import ArtifactRef  # noqa: E402
from nutrition_data_factory.impact import build_impact_report  # noqa: E402
from nutrition_data_factory.models import SourceMetadata, ValidationReport  # noqa: E402
from nutrition_data_factory.release.compiler import (  # noqa: E402
    ReleaseCompilationError,
    compile_candidate_package,
)


class ReleaseImpactTests(unittest.TestCase):
    def test_impact_reports_all_categories_provenance_coverage_and_material_flags(self) -> None:
        previous = {
            "identities": [{"id": "food-1"}],
            "names": [{"id": "name-1", "value": "Alpha"}],
            "mappings": [],
            "profiles": [{"id": "profile-1"}],
            "values": [{"value_id": "value-1", "value": 10, "provenance": {"release": "old"}}],
            "recipes": [],
            "portions": [],
        }
        current = {
            "identities": [{"id": "food-1"}, {"id": "food-2"}],
            "names": [{"id": "name-1", "value": "Beta"}],
            "mappings": [{"id": "mapping-1"}],
            "profiles": [{"id": "profile-1"}],
            "values": [{"value_id": "value-1", "value": 20, "provenance": {"release": "new"}}],
            "recipes": [],
            "portions": [],
        }
        report = build_impact_report(
            previous,
            current,
            benchmark_cases=[
                {"case_id": "bench-1", "candidate_id": "food-1", "evidence_class": "analytical"},
                {"case_id": "bench-2", "candidate_id": "unknown", "evidence_class": "analytical"},
            ],
            fixed_meal_cases=[
                {"case_id": "meal-1", "items": [{"item_key": "energy", "old_value": 10, "new_value": 20, "old_provenance": "old", "new_provenance": "new"}]}
            ],
        )
        value_change = report.to_dict()["changes"]["names"]["changed"]
        self.assertEqual(value_change[0]["old"]["value"], "Alpha")
        self.assertEqual(report.nutrient_deltas[0]["old_provenance"], {"release": "old"})
        self.assertEqual(report.coverage["by_evidence_class"]["analytical"]["covered"], 1)
        self.assertEqual(report.material_delta_flags[0]["reason_code"], "material_nutrient_delta")
        self.assertFalse(report.to_dict()["clinical_correctness_claimed"])

    def test_compiler_requires_external_owner_approval_to_change_production_eligibility(self) -> None:
        metadata = SourceMetadata(
            code="usda_fdc_foundation",
            publisher="USDA ARS",
            purpose="synthetic test metadata",
            release="2026-04-30",
            locator="fixture://fdc",
            status_initial="approved_candidate",
            rights_state="approved",
            production_ingestion="allowed_after_project_gates",
            access_status="official_download",
            approval_reference="fixture-approval",
            allowed_uses=("staged_candidate",),
            prohibited_uses=(),
            priority=1,
            production_eligible=False,
        )
        validation = ValidationReport("test", True, (), (), {})
        artifact = ArtifactRef("a" * 64, 0, "application/json", "aa/" + "a" * 62)
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(ReleaseCompilationError):
                compile_candidate_package(
                    Path(temporary) / "without-approval",
                    metadata,
                    artifact,
                    [],
                    [],
                    validation,
                    production_eligible=True,
                )
            output = compile_candidate_package(
                Path(temporary) / "with-approval",
                metadata,
                artifact,
                [],
                [],
                validation,
                production_eligible=True,
                owner_approval_ref="OWNER-DATA-008-test-only",
            )
            self.assertTrue(__import__("json").loads((output / "manifest.json").read_text())["production_eligible"])


if __name__ == "__main__":
    unittest.main()

