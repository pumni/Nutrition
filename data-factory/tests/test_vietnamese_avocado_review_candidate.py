from __future__ import annotations

import hashlib
import json
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
PACKET_PATH = ROOT / "docs" / "reviews" / "vietnamese-avocado-issue-35-review-candidate-0.1.0.json"
BENCHMARK_PATH = ROOT.parent / "backend" / "fixtures" / "vietnamese-meal-bench" / "public-test-cases.json"
SOURCE_REGISTRY_PATH = ROOT / "config" / "source_registry.json"
EXPECTED_PACKET_SHA256 = "4a9e18ea762f5b76233629d8d416df1ee048281b2368176066d37b45ea08dd49"


class VietnameseAvocadoReviewCandidateTests(unittest.TestCase):
    def test_candidate_packet_pins_one_pending_human_review_case(self) -> None:
        packet_bytes = PACKET_PATH.read_bytes()
        packet: dict[str, Any] = json.loads(packet_bytes)
        self.assertEqual(hashlib.sha256(packet_bytes).hexdigest(), EXPECTED_PACKET_SHA256)
        self.assertEqual(packet["packet_version"], "fndds-source-review-packet-0.1.0")
        self.assertEqual(packet["issue_reference"], "github:issue/35")
        self.assertEqual(packet["proposal_id"], "fndds-proposal-ca32d18c753ee4aaf638")

        self.assertEqual(packet["target_id"], "vmb-public-0004")
        self.assertEqual(packet["normalized_vietnamese_target"], "bơ")
        self.assertEqual(packet["mapping_scope"], "vmb-public-0004 only; no global alias approval")

        benchmark_bytes = BENCHMARK_PATH.read_bytes()
        self.assertEqual(
            hashlib.sha256(benchmark_bytes).hexdigest(),
            packet["benchmark_context"]["file_sha256"],
        )
        cases = json.loads(benchmark_bytes)
        case = next(item for item in cases if item["sample_id"] == "vmb-public-0004")
        self.assertEqual(case["text"], packet["benchmark_context"]["source_text"])
        self.assertEqual(case["adjudication_status"], "pending_human_review")
        benchmark_item = case["expected_parse"]["items"][0]
        self.assertEqual(benchmark_item["food_phrase"], "bơ")
        self.assertEqual(benchmark_item["quantity"], "0.5")
        self.assertEqual(benchmark_item["unit"], "quả")
        self.assertEqual(packet["benchmark_adjudication_status"], "pending_human_review")
        self.assertFalse(packet["reviewer_approved"])
        self.assertEqual(packet["reviewer_state"], "pending_human_review")
        self.assertIsNone(packet["reviewer_reference"])

    def test_source_record_and_nutrient_completeness_are_exact_and_nonproduction(self) -> None:
        packet: dict[str, Any] = json.loads(PACKET_PATH.read_bytes())
        source_record = packet["source_record"]
        self.assertEqual(source_record["source_code"], "usda_fndds")
        self.assertEqual(source_record["fndds_food_code"], "63105010")
        self.assertEqual(source_record["fdc_id"], 2709223)
        self.assertEqual(source_record["description"], "Avocado, raw")
        self.assertEqual(
            source_record["record_sha256"],
            "93c417a29518c43b95d11809bb11cc915d3aa6ffb1db13578a7578139b18a508",
        )

        source_artifact = packet["source_artifact"]
        self.assertEqual(source_artifact["source_code"], "usda_fndds")
        self.assertEqual(
            source_artifact["archive_sha256"],
            "dfb06ae7ddc397ccd570b91c14b75438ab2ba39f64f22d321f61d4a52a77f3eb",
        )
        self.assertEqual(
            source_artifact["member_sha256"],
            "2e7eb9fda92adf1d4d784dba5eaa3a7fd4418cd86ccff383c7c9294d79e9b808",
        )
        self.assertEqual(
            source_artifact["schema_fingerprint"],
            "bc072c4e5cf81ebb632ddb099d502e4e8a5cfef55933b9a18a60840a86e6e4f4",
        )

        nutrients = packet["required_nutrient_completeness"]
        self.assertTrue(nutrients["complete"])
        self.assertEqual(
            {
                item["target_code"]: (
                    item["fndds_nutrient_code"],
                    item["source_nutrient_id"],
                    item["source_unit"],
                )
                for item in nutrients["nutrients"]
            },
            {
                "energy_kcal": ("208", 1008, "kcal"),
                "protein_g": ("203", 1003, "g"),
                "carbohydrate_g": ("205", 1005, "g"),
                "fat_g": ("204", 1004, "g"),
            },
        )

        registry: dict[str, Any] = json.loads(SOURCE_REGISTRY_PATH.read_bytes())
        fndds = next(item for item in registry["sources"] if item["code"] == "usda_fndds")
        self.assertEqual(fndds["rights_state"], "reference_only")
        self.assertEqual(fndds["production_ingestion"], "blocked_until_product_selection")
        self.assertEqual(fndds["allowed_uses"], ["analysis", "reference"])
        self.assertIn("staged_candidate", fndds["prohibited_uses"])
        self.assertFalse(fndds["production_eligible"])
        self.assertFalse(packet["catalog_staging_authorized"])
        self.assertFalse(packet["production_eligible"])
        self.assertFalse(packet["activation_authorized"])
        self.assertFalse(packet["portion_evidence_authorized"])
        self.assertFalse(packet["recipe_evidence_authorized"])

        serialized = json.dumps(packet, ensure_ascii=False, sort_keys=True)
        for forbidden in ("foodPortions", "gramWeight", "mass_g", "actual_masses", "source_amount"):
            self.assertNotIn(forbidden, serialized)


if __name__ == "__main__":
    unittest.main()
