from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BENCHMARK_PATH = ROOT.parent / "backend" / "fixtures" / "vietnamese-meal-bench" / "public-test-cases.json"
SOURCE_REGISTRY_PATH = ROOT / "config" / "source_registry.json"
VERSIONED_POLICY_PATH = ROOT / "config" / "fndds-secondary-review-policy-0.2.0.json"
VERSIONED_PACKET_PATH = (
    ROOT / "docs" / "reviews" / "vietnamese-avocado-issue-35-review-candidate-0.2.0.json"
)
AVOCADO_SOURCE_FIXTURE_PATH = ROOT / "tests" / "fixtures" / "fndds-avocado-review.source.json"
EXPECTED_VERSIONED_PACKET_SHA256 = "2ef63070571a90bb11f9e795ffa23bc3157687885fcb46c77898be4103296050"

sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.adapters.fndds_survey import (  # noqa: E402
    FNDDS_ADAPTER_VERSION,
    FNDDS_RELEASE,
    FnddsSurveyAdapter,
)
from nutrition_data_factory.curation.fndds_review import (  # noqa: E402
    FNDDS_AVOCADO_REVIEW_POLICY_VERSION,
    FnddsReviewError,
    build_fndds_review_report,
    load_fndds_review_policy,
    serialize_fndds_review_artifact,
)
from nutrition_data_factory.source_registry import SourceRegistry  # noqa: E402


class VietnameseAvocadoReviewCandidateTests(unittest.TestCase):
    def _fixture_generation_inputs(self):
        loaded_policy = load_fndds_review_policy(VERSIONED_POLICY_PATH)
        policy = loaded_policy["policy"]
        fixture_bytes = AVOCADO_SOURCE_FIXTURE_PATH.read_bytes()
        fixture_parse = FnddsSurveyAdapter().parse(fixture_bytes, release=FNDDS_RELEASE)
        self.assertTrue(fixture_parse.valid)
        self.assertEqual(fixture_parse.raw_record_count, 3)

        # Rows are verbatim excerpts from the pinned release. The real archive command performs
        # full-payload validation; this fixture keeps candidate packet generation offline in CI.
        parsed = replace(
            fixture_parse,
            source_sha256=policy["source"]["member_sha256"],
            schema_fingerprint=policy["source"]["schema_fingerprint"],
        )
        metadata = SourceRegistry.load(SOURCE_REGISTRY_PATH).get("usda_fndds")
        source_use_boundary = {
            "rights_state": metadata.rights_state,
            "production_ingestion": metadata.production_ingestion,
            "allowed_uses": list(metadata.allowed_uses),
            "prohibited_uses": list(metadata.prohibited_uses),
            "production_eligible": metadata.production_eligible,
        }
        source_evidence = {
            "archive_filename": policy["source"]["archive_filename"],
            "archive_sha256": policy["source"]["archive_sha256"],
            "archive_size_bytes": policy["source"]["archive_size_bytes"],
            "member_name": policy["source"]["archive_member"],
            "member_sha256": policy["source"]["member_sha256"],
            "member_size_bytes": policy["source"]["member_size_bytes"],
            "schema_fingerprint": policy["source"]["schema_fingerprint"],
            "source_use_boundary": source_use_boundary,
        }
        return loaded_policy, parsed, source_evidence, source_use_boundary

    def test_versioned_scope_generates_the_committed_packet_from_source_records(self) -> None:
        loaded_policy, parsed, source_evidence, source_use_boundary = self._fixture_generation_inputs()
        policy = loaded_policy["policy"]
        self.assertEqual(policy["schema_version"], FNDDS_AVOCADO_REVIEW_POLICY_VERSION)
        self.assertEqual(
            {target["target_id"] for target in policy["targets"]},
            {"vmb-public-0004"},
        )

        report = build_fndds_review_report(parsed, loaded_policy, source_evidence)
        packet = report["candidate_packets"][0]
        generated_bytes = serialize_fndds_review_artifact(packet)
        self.assertEqual(generated_bytes, VERSIONED_PACKET_PATH.read_bytes())
        self.assertEqual(
            hashlib.sha256(generated_bytes).hexdigest(),
            EXPECTED_VERSIONED_PACKET_SHA256,
        )

        self.assertEqual(packet["packet_version"], "fndds-source-review-packet-0.2.0")
        self.assertEqual(packet["review_policy_version"], policy["schema_version"])
        self.assertEqual(
            packet["review_policy_sha256"],
            hashlib.sha256(VERSIONED_POLICY_PATH.read_bytes()).hexdigest(),
        )
        self.assertEqual(packet["source_adapter_version"], FNDDS_ADAPTER_VERSION)
        self.assertEqual(packet["proposal_id"], "fndds-proposal-b27df76ad6826adc38d1")
        self.assertEqual(packet["target_id"], "vmb-public-0004")
        self.assertEqual(packet["normalized_vietnamese_target"], "bơ")
        self.assertEqual(packet["mapping_scope"], "vmb-public-0004 only; no global alias approval")

        source_record = packet["source_record"]
        self.assertEqual(source_record["source_code"], "usda_fndds")
        self.assertEqual(source_record["fndds_food_code"], "63105010")
        self.assertEqual(source_record["fdc_id"], 2709223)
        self.assertEqual(source_record["description"], "Avocado, raw")
        self.assertEqual(
            source_record["wweia_category_description"], "Other vegetables and combinations"
        )
        self.assertEqual(
            source_record["record_sha256"],
            "93c417a29518c43b95d11809bb11cc915d3aa6ffb1db13578a7578139b18a508",
        )
        completeness = packet["required_nutrient_completeness"]
        self.assertTrue(completeness["complete"])
        self.assertEqual(
            {
                item["target_code"]: (
                    item["fndds_nutrient_code"],
                    item["source_nutrient_id"],
                    item["source_unit"],
                )
                for item in completeness["required_nutrients"]
            },
            {
                "energy_kcal": ("208", 1008, "kcal"),
                "protein_g": ("203", 1003, "g"),
                "carbohydrate_g": ("205", 1005, "g"),
                "fat_g": ("204", 1004, "g"),
            },
        )
        self.assertEqual(
            packet["source_artifact"]["archive_sha256"],
            "dfb06ae7ddc397ccd570b91c14b75438ab2ba39f64f22d321f61d4a52a77f3eb",
        )
        self.assertEqual(
            packet["source_artifact"]["archive_filename"],
            "FoodData_Central_survey_food_json_2024-10-31.zip",
        )
        self.assertEqual(
            packet["source_artifact"]["member_sha256"],
            "2e7eb9fda92adf1d4d784dba5eaa3a7fd4418cd86ccff383c7c9294d79e9b808",
        )
        self.assertEqual(
            packet["source_artifact"]["schema_fingerprint"],
            policy["source"]["schema_fingerprint"],
        )
        self.assertEqual(packet["source_artifact"]["release"], "2021-2023")
        self.assertEqual(packet["source_artifact"]["release_date"], "2024-10")

        self.assertEqual(packet["reviewer_state"], "pending_human_review")
        self.assertFalse(packet["reviewer_approved"])
        self.assertIsNone(packet["reviewer_reference"])
        self.assertEqual(packet["benchmark_adjudication_status"], "pending_human_review")
        self.assertEqual(packet["issue_reference"], "github:issue/35")
        self.assertEqual(
            packet["source_artifact"]["project_decision_reference"],
            "github:issue/41#issuecomment-5888205397",
        )
        self.assertEqual(packet["foundation_primary_check"]["source"], "usda_fdc_foundation")
        self.assertFalse(packet["foundation_primary_check"]["foundation_exact_mapping_accepted"])
        foundation_row = packet["foundation_primary_check"]["observed_source_record"]
        self.assertEqual(foundation_row["description"], "Avocado, Hass, peeled, raw")
        self.assertEqual(foundation_row["selection_state"], "not_selected_cultivar_specific")

        benchmark_bytes = BENCHMARK_PATH.read_bytes()
        self.assertEqual(
            hashlib.sha256(benchmark_bytes).hexdigest(),
            packet["benchmark_context"]["file_sha256"],
        )
        benchmark_case = next(
            item
            for item in json.loads(benchmark_bytes)
            if item["sample_id"] == "vmb-public-0004"
        )
        self.assertEqual(benchmark_case["text"], "1/2 quả bơ")
        self.assertEqual(benchmark_case["adjudication_status"], "pending_human_review")

        self.assertEqual(packet["source_use_boundary"], source_use_boundary)
        self.assertEqual(source_use_boundary["rights_state"], "reference_only")
        self.assertEqual(source_use_boundary["production_ingestion"], "blocked_until_product_selection")
        self.assertEqual(source_use_boundary["allowed_uses"], ["analysis", "reference"])
        self.assertIn("staged_candidate", source_use_boundary["prohibited_uses"])
        self.assertFalse(source_use_boundary["production_eligible"])
        for flag in (
            "catalog_staging_authorized",
            "production_eligible",
            "activation_authorized",
            "portion_evidence_authorized",
            "recipe_evidence_authorized",
        ):
            self.assertFalse(packet[flag])

        packet_bytes_text = generated_bytes.decode("utf-8")
        for forbidden in ("foodPortions", "gramWeight", "mass_g", "source_amount"):
            self.assertNotIn(forbidden, packet_bytes_text)

    def test_authorized_target_and_owner_decision_cannot_drift(self) -> None:
        document = json.loads(VERSIONED_POLICY_PATH.read_bytes())
        policy_mutations = (
            lambda value: value["targets"][0].__setitem__("normalized_vietnamese_target", "avocado"),
            lambda value: value["targets"][0].__setitem__("target_id", "vmb-public-0003"),
            lambda value: value.__setitem__("project_decision_reference", "github:issue/41"),
        )
        with tempfile.TemporaryDirectory() as temporary_directory:
            for mutate in policy_mutations:
                changed = json.loads(json.dumps(document))
                mutate(changed)
                policy_path = Path(temporary_directory) / "scope.json"
                policy_path.write_text(
                    json.dumps(changed, ensure_ascii=False), encoding="utf-8"
                )
                with self.assertRaises(FnddsReviewError):
                    load_fndds_review_policy(policy_path)

    def test_source_identity_and_required_nutrient_drift_fail_closed(self) -> None:
        loaded_policy, parsed, source_evidence, _ = self._fixture_generation_inputs()
        policy_mutations = (
            (lambda value: value["candidate"].__setitem__("fndds_food_code", "63105011"), "source record"),
            (lambda value: value["candidate"].__setitem__("fdc_id", 2709224), "source ID"),
            (lambda value: value["candidate"].__setitem__("description", "Avocado, cooked"), "source ID"),
            (lambda value: value["candidate"].__setitem__("record_sha256", "0" * 64), "record hash"),
            (
                lambda value: value["candidate"]["required_nutrient_semantics"][1].__setitem__(
                    "source_nutrient_id", 2047
                ),
                "required nutrient semantics",
            ),
        )
        for mutate, message in policy_mutations:
            with self.subTest(message=message):
                changed_policy = json.loads(json.dumps(loaded_policy))
                mutate(changed_policy["policy"]["targets"][0])
                with self.assertRaisesRegex(FnddsReviewError, message):
                    build_fndds_review_report(parsed, changed_policy, source_evidence)

        override_policy = json.loads(json.dumps(loaded_policy))
        override_policy["policy"]["targets"][0]["review_context"]["reviewer_approved"] = True
        override_packet = build_fndds_review_report(
            parsed, override_policy, source_evidence
        )["candidate_packets"][0]
        self.assertFalse(override_packet["reviewer_approved"])

        source_mutations = (
            (
                lambda row: row.__setitem__("foodCode", "63105011"),
                "source record",
            ),
            (lambda row: row.__setitem__("fdcId", 2709224), "source ID"),
            (lambda row: row.__setitem__("description", "Avocado, cooked"), "source ID"),
            (lambda row: row["foodNutrients"].pop(), "record hash"),
        )
        for mutate, message in source_mutations:
            with self.subTest(source_drift=message):
                changed_source = json.loads(AVOCADO_SOURCE_FIXTURE_PATH.read_bytes())
                mutate(changed_source["SurveyFoods"][0])
                changed_parse = FnddsSurveyAdapter().parse(
                    json.dumps(changed_source, ensure_ascii=False).encode("utf-8"),
                    release=FNDDS_RELEASE,
                )
                changed_evidence = {
                    **source_evidence,
                    "member_sha256": changed_parse.source_sha256,
                    "schema_fingerprint": changed_parse.schema_fingerprint,
                }
                with self.assertRaisesRegex(FnddsReviewError, message):
                    build_fndds_review_report(changed_parse, loaded_policy, changed_evidence)

        changed_source = json.loads(AVOCADO_SOURCE_FIXTURE_PATH.read_bytes())
        energy = next(
            item
            for item in changed_source["SurveyFoods"][0]["foodNutrients"]
            if item["nutrient"]["number"] == "208"
        )
        energy["nutrient"]["id"] = 2047
        changed_parse = FnddsSurveyAdapter().parse(
            json.dumps(changed_source, ensure_ascii=False).encode("utf-8"),
            release=FNDDS_RELEASE,
        )
        changed_policy = json.loads(json.dumps(loaded_policy))
        changed_policy["policy"]["targets"][0]["candidate"]["record_sha256"] = next(
            item.payload_sha256
            for item in changed_parse.accepted_records
            if item.food_code == "63105010"
        )
        with self.assertRaisesRegex(FnddsReviewError, "required nutrient semantics"):
            build_fndds_review_report(
                changed_parse,
                changed_policy,
                {
                    **source_evidence,
                    "member_sha256": changed_parse.source_sha256,
                    "schema_fingerprint": changed_parse.schema_fingerprint,
                },
            )


if __name__ == "__main__":
    unittest.main()
