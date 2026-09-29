from __future__ import annotations

import copy
import hashlib
import json
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.adapters.fndds_survey import (  # noqa: E402
    FNDDS_ADAPTER_VERSION,
    FnddsSourceRecord,
)
from nutrition_data_factory.curation.reviewed_composition import (  # noqa: E402
    REVIEWED_COMPOSITION_POLICY_VERSION,
    REVIEWED_COMPOSITION_SCHEMA_VERSION,
    REVIEWED_MAPPING_SHA256,
    ReviewedCompositionError,
    build_reviewed_rice_composition,
    validate_fndds_source_policy,
    write_immutable_artifact,
)
from nutrition_data_factory.curation.fndds_review import load_fndds_review_policy  # noqa: E402
from nutrition_data_factory.nutrients_fndds import (  # noqa: E402
    FNDDS_NUTRIENT_MAPPING_VERSION,
    extract_fndds_required_nutrient_values,
)
from nutrition_data_factory.source_registry import SourceRegistry  # noqa: E402
from test_reviewed_mapping import _assert_json_schema_conforms  # noqa: E402


MAPPING_PATH = ROOT / "docs" / "reviews" / "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json"
COMMITTED_ARTIFACT_PATH = ROOT / "docs" / "reviews" / "vietnamese-com-trang-reviewed-composition-0.1.0.json"
SCHEMA_PATH = ROOT / "schemas" / "reviewed-food-composition-0.1.0.schema.json"
SOURCE_REGISTRY_PATH = ROOT / "config" / "source_registry.json"
POLICY_PATH = ROOT / "config" / "fndds-secondary-review-policy.json"
EXPECTED_SOURCE_RECORD_SHA256 = "634cf6fabbcc9ccb76f513ef274a9dcc3d92f495c78792cf8221170d98a102cc"


class ReviewedCompositionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.mapping_bytes = MAPPING_PATH.read_bytes()
        self.registry = SourceRegistry.load(SOURCE_REGISTRY_PATH)
        self.archive_evidence = {
            "archive_sha256": "dfb06ae7ddc397ccd570b91c14b75438ab2ba39f64f22d321f61d4a52a77f3eb",
            "archive_member": "surveyDownload.json",
            "member_sha256": "2e7eb9fda92adf1d4d784dba5eaa3a7fd4418cd86ccff383c7c9294d79e9b808",
            "schema_fingerprint": "bc072c4e5cf81ebb632ddb099d502e4e8a5cfef55933b9a18a60840a86e6e4f4",
        }

    def test_synthetic_record_materializes_exactly_four_required_values(self) -> None:
        artifact = json.loads(
            build_reviewed_rice_composition(
                _synthetic_record(), self.mapping_bytes, self.archive_evidence, self.registry
            )
        )
        self.assertEqual(artifact["schema_version"], REVIEWED_COMPOSITION_SCHEMA_VERSION)
        self.assertEqual(artifact["policy_version"], REVIEWED_COMPOSITION_POLICY_VERSION)
        self.assertEqual(artifact["reviewed_mapping_sha256"], REVIEWED_MAPPING_SHA256)
        self.assertEqual(artifact["source_record_sha256"], EXPECTED_SOURCE_RECORD_SHA256)
        self.assertEqual(
            {
                item["target_code"]: (
                    item["fndds_nutrient_code"],
                    item["source_nutrient_id"],
                    item["source_unit"],
                    item["canonical_unit"],
                )
                for item in artifact["nutrients"]
            },
            {
                "energy_kcal": ("208", 1008, "kcal", "kcal"),
                "protein_g": ("203", 1003, "g", "g"),
                "carbohydrate_g": ("205", 1005, "g", "g"),
                "fat_g": ("204", 1004, "g", "g"),
            },
        )
        self.assertEqual(artifact["basis_amount"], 100)
        self.assertEqual(artifact["basis_unit"], "g")
        self.assertIs(artifact["edible_basis"], True)

    def test_mapping_hash_drift_fails_closed(self) -> None:
        with self.assertRaisesRegex(ReviewedCompositionError, "mapping SHA-256"):
            build_reviewed_rice_composition(
                _synthetic_record(),
                self.mapping_bytes + b" ",
                self.archive_evidence,
                self.registry,
            )

    def test_wrong_food_code_or_fdc_id_fails_closed(self) -> None:
        for changed in (
            replace(_synthetic_record(), food_code="56205001"),
            replace(_synthetic_record(), fdc_id=2708403),
        ):
            with self.subTest(record=changed):
                with self.assertRaisesRegex(ReviewedCompositionError, "record identity"):
                    build_reviewed_rice_composition(
                        changed, self.mapping_bytes, self.archive_evidence, self.registry
                    )

    def test_source_record_hash_drift_fails_closed(self) -> None:
        changed = replace(_synthetic_record(), payload_sha256="0" * 64)
        with self.assertRaisesRegex(ReviewedCompositionError, "source hash"):
            build_reviewed_rice_composition(
                changed, self.mapping_bytes, self.archive_evidence, self.registry
            )

    def test_archive_member_and_schema_hash_drift_fails_closed(self) -> None:
        for key in ("archive_sha256", "member_sha256", "schema_fingerprint"):
            evidence = dict(self.archive_evidence)
            evidence[key] = "0" * 64
            with self.subTest(key=key):
                with self.assertRaisesRegex(ReviewedCompositionError, "archive/member/schema"):
                    build_reviewed_rice_composition(
                        _synthetic_record(), self.mapping_bytes, evidence, self.registry
                    )

    def test_missing_duplicate_wrong_id_wrong_code_unit_and_null_values_fail(self) -> None:
        nutrients = list(_synthetic_record().nutrients)
        mutations = {
            "missing": nutrients[:-1],
            "duplicate": nutrients + [copy.deepcopy(nutrients[0])],
        }
        wrong_id = copy.deepcopy(nutrients)
        wrong_id[0]["nutrient"]["id"] = 9876
        mutations["wrong source ID"] = wrong_id
        wrong_code = copy.deepcopy(nutrients)
        wrong_code[1]["nutrient"]["number"] = "999"
        mutations["wrong FNDDS code"] = wrong_code
        wrong_unit = copy.deepcopy(nutrients)
        wrong_unit[1]["nutrient"]["unitName"] = "mg"
        mutations["wrong source unit"] = wrong_unit
        null_amount = copy.deepcopy(nutrients)
        null_amount[2]["amount"] = None
        mutations["null amount"] = null_amount

        for name, changed_nutrients in mutations.items():
            with self.subTest(name=name):
                changed = replace(_synthetic_record(), nutrients=tuple(changed_nutrients))
                with self.assertRaises(ReviewedCompositionError):
                    build_reviewed_rice_composition(
                        changed, self.mapping_bytes, self.archive_evidence, self.registry
                    )

    def test_decimal_format_is_canonical_and_never_rounds(self) -> None:
        record = _synthetic_record(
            amounts=(1.2300, 2.5000, 3.75, 4.1250)
        )
        extracted = extract_fndds_required_nutrient_values(record)
        self.assertEqual(
            {item["target_code"]: item["source_amount"] for item in extracted},
            {
                "energy_kcal": "1.23",
                "protein_g": "2.5",
                "carbohydrate_g": "3.75",
                "fat_g": "4.125",
            },
        )
        self.assertEqual(
            extract_fndds_required_nutrient_values(
                _synthetic_record(amounts=(1.0, 1000.0, 0.00012, 0.0))
            )[0]["source_amount"],
            "0.00012",
        )

    def test_output_is_deterministic_and_omits_portion_and_recipe_evidence(self) -> None:
        record = _synthetic_record()
        first = build_reviewed_rice_composition(
            record, self.mapping_bytes, self.archive_evidence, self.registry
        )
        second = build_reviewed_rice_composition(
            record, self.mapping_bytes, self.archive_evidence, self.registry
        )
        self.assertEqual(first, second)
        encoded = first.decode("utf-8")
        self.assertNotIn("foodPortions", encoded)
        self.assertNotIn("gramWeight", encoded)
        self.assertNotIn("household", encoded)
        self.assertNotIn("portion_mass", encoded)
        artifact = json.loads(first)
        self.assertNotIn("recipe_evidence", artifact)
        self.assertIs(artifact["evaluation_eligible"], True)
        self.assertIs(artifact["catalog_staging_authorized"], False)
        self.assertIs(artifact["production_eligible"], False)
        self.assertIs(artifact["activation_authorized"], False)
        self.assertIs(artifact["portion_evidence_authorized"], False)
        self.assertIs(artifact["recipe_evidence_authorized"], False)
        self.assertNotIn("synthetic portion sentinel", encoded)

    def test_current_fndds_registry_boundary_is_narrow_and_unrelated_edits_do_not_change_output(self) -> None:
        policy = load_fndds_review_policy(POLICY_PATH)
        source = validate_fndds_source_policy(policy["policy"])
        self.assertEqual(source["rights_state"], "reference_only")
        self.assertEqual(source["source_code"], "usda_fndds")
        metadata = self.registry.get("usda_fndds")
        self.assertEqual(set(metadata.allowed_uses), {"analysis", "reference"})
        self.assertIn("staged_candidate", metadata.prohibited_uses)
        self.assertIs(metadata.production_eligible, False)

        original = build_reviewed_rice_composition(
            _synthetic_record(), self.mapping_bytes, self.archive_evidence, self.registry
        )
        registry_data = json.loads(SOURCE_REGISTRY_PATH.read_bytes())
        fixture = next(item for item in registry_data["sources"] if item["code"] == "synthetic_fixture")
        fixture["purpose"] = "unrelated mutable source metadata"
        with tempfile.TemporaryDirectory() as directory:
            changed_path = Path(directory) / "source_registry.json"
            changed_path.write_text(json.dumps(registry_data), encoding="utf-8")
            changed_registry = SourceRegistry.load(changed_path)
        changed_output = build_reviewed_rice_composition(
            _synthetic_record(), self.mapping_bytes, self.archive_evidence, changed_registry
        )
        self.assertEqual(original, changed_output)

    def test_source_wide_staging_permission_change_fails_closed(self) -> None:
        registry_data = json.loads(SOURCE_REGISTRY_PATH.read_bytes())
        fndds = next(item for item in registry_data["sources"] if item["code"] == "usda_fndds")
        fndds["allowed_uses"].append("staged_candidate")
        with tempfile.TemporaryDirectory() as directory:
            changed_path = Path(directory) / "source_registry.json"
            changed_path.write_text(json.dumps(registry_data), encoding="utf-8")
            changed_registry = SourceRegistry.load(changed_path)
        with self.assertRaisesRegex(ReviewedCompositionError, "registry safety policy"):
            build_reviewed_rice_composition(
                _synthetic_record(), self.mapping_bytes, self.archive_evidence, changed_registry
            )

    def test_refuses_conflicting_artifact_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "artifact.json"
            output.write_bytes(b"previous immutable evidence")
            with self.assertRaisesRegex(ReviewedCompositionError, "refusing to overwrite"):
                write_immutable_artifact(output, b"new evidence")

    def test_committed_artifact_conforms_to_schema_and_has_stable_hash(self) -> None:
        artifact_bytes = COMMITTED_ARTIFACT_PATH.read_bytes()
        artifact = json.loads(artifact_bytes)
        schema = json.loads(SCHEMA_PATH.read_bytes())
        _assert_json_schema_conforms(artifact, schema)
        self.assertEqual(
            hashlib.sha256(artifact_bytes).hexdigest(),
            "f1d0434af02bc18f128e0f4910464971d7ac34690f4e5cf14bce85e26de5410c",
        )
        self.assertEqual(artifact["nutrient_mapping_version"], FNDDS_NUTRIENT_MAPPING_VERSION)
        self.assertEqual(artifact["fndds_adapter_version"], FNDDS_ADAPTER_VERSION)


def _synthetic_record(
    *, amounts: tuple[float | int, float | int, float | int, float | int] = (1.25, 2.5, 3.75, 4.125)
) -> FnddsSourceRecord:
    # Synthetic amounts and portion sentinel exercise transformation only; never use these values
    # to generate the committed evaluation artifact.
    definitions = (
        ("208", 1008, "Energy", "kcal"),
        ("203", 1003, "Protein", "g"),
        ("205", 1005, "Carbohydrate", "g"),
        ("204", 1004, "Fat", "g"),
    )
    nutrients = tuple(
        {
            "nutrient": {"number": code, "id": source_id, "name": label, "unitName": unit},
            "amount": amount,
        }
        for (code, source_id, label, unit), amount in zip(definitions, amounts, strict=True)
    )
    return FnddsSourceRecord(
        food_code="56205008",
        fdc_id=2708408,
        description="Rice, white, cooked, no added fat",
        data_type="Survey (FNDDS)",
        wweia_category_description="Rice",
        nutrients=nutrients,
        payload={"foodNutrients": list(nutrients), "foodPortions": [{"gramWeight": "synthetic portion sentinel"}]},
        payload_sha256=EXPECTED_SOURCE_RECORD_SHA256,
    )


if __name__ == "__main__":
    unittest.main()
