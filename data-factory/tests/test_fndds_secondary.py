from __future__ import annotations

import copy
import hashlib
import io
import json
import sys
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.adapters.fndds_survey import (  # noqa: E402
    FNDDS_RELEASE,
    FnddsSurveyAdapter,
    extract_pinned_archive_member,
)
from nutrition_data_factory.curation.fndds_review import (  # noqa: E402
    FnddsReviewError,
    build_fndds_review_report,
    load_fndds_review_policy,
)
from nutrition_data_factory.nutrients_fndds import (  # noqa: E402
    FNDDS_NUTRIENT_MAPPING_VERSION,
    summarize_fndds_required_nutrients,
)


POLICY_PATH = ROOT / "config" / "fndds-secondary-review-policy.json"
FIXTURE_PATH = ROOT / "tests" / "fixtures" / "fndds-2021-2023-reviewed-subset.source.json"


class FnddsSecondaryTests(unittest.TestCase):
    # This is an exact row excerpt from the pinned USDA archive (archive SHA-256:
    # dfb06ae7ddc397ccd570b91c14b75438ab2ba39f64f22d321f61d4a52a77f3eb).
    # Source portions remain in the fixture only to prove they do not enter packets.
    def setUp(self) -> None:
        self.policy = load_fndds_review_policy(POLICY_PATH)
        self.payload = FIXTURE_PATH.read_bytes()
        self.parsed = FnddsSurveyAdapter().parse(
            self.payload,
            release=FNDDS_RELEASE,
            expected_sha256=hashlib.sha256(self.payload).hexdigest(),
        )
        self.source_evidence = {
            "archive_filename": "test-fixture-source-excerpt.json",
            "archive_sha256": None,
            "member_name": "SurveyFoods test excerpt",
            "member_sha256": self.parsed.source_sha256,
            "schema_fingerprint": self.parsed.schema_fingerprint,
        }

    def test_release_and_checksum_pins_fail_closed(self) -> None:
        self.assertEqual(self.parsed.release, "2021-2023")
        self.assertEqual(self.parsed.raw_record_count, 15)
        self.assertEqual(len(self.parsed.accepted_records), 15)
        self.assertFalse(self.parsed.rejected_records)

        with self.assertRaisesRegex(ValueError, "release"):
            FnddsSurveyAdapter().parse(self.payload, release="2019-2020")
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            FnddsSurveyAdapter().parse(self.payload, expected_sha256="0" * 64)

        source = self.policy["policy"]["source"]
        self.assertEqual(
            source["archive_filename"],
            "FoodData_Central_survey_food_json_2024-10-31.zip",
        )
        self.assertEqual(
            source["archive_sha256"],
            "dfb06ae7ddc397ccd570b91c14b75438ab2ba39f64f22d321f61d4a52a77f3eb",
        )
        self.assertEqual(
            source["member_sha256"],
            "2e7eb9fda92adf1d4d784dba5eaa3a7fd4418cd86ccff383c7c9294d79e9b808",
        )
        with self.assertRaisesRegex(ValueError, "archive SHA-256"):
            extract_pinned_archive_member(
                b"not the pinned USDA archive",
                source["archive_filename"],
                source,
            )
        with self.assertRaisesRegex(ValueError, "filename"):
            extract_pinned_archive_member(b"anything", "different-release.zip", source)

    def test_archive_member_is_hash_checked_after_exact_archive_pin(self) -> None:
        member = b'{"SurveyFoods":[]}'
        archive_buffer = io.BytesIO()
        with zipfile.ZipFile(archive_buffer, mode="w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("surveyDownload.json", member)
        archive_bytes = archive_buffer.getvalue()
        fixture_pin = {
            "archive_filename": "structure-only.zip",
            "archive_sha256": hashlib.sha256(archive_bytes).hexdigest(),
            "archive_member": "surveyDownload.json",
            "member_sha256": hashlib.sha256(member).hexdigest(),
        }
        self.assertEqual(
            extract_pinned_archive_member(archive_bytes, "structure-only.zip", fixture_pin),
            member,
        )
        fixture_pin["member_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "member SHA-256"):
            extract_pinned_archive_member(archive_bytes, "structure-only.zip", fixture_pin)

    def test_required_nutrient_crosswalk_is_explicit_and_source_specific(self) -> None:
        self.assertEqual(FNDDS_NUTRIENT_MAPPING_VERSION, "fndds-required-nutrients-0.1.0")
        rice = next(record for record in self.parsed.accepted_records if record.food_code == "56205008")
        result = summarize_fndds_required_nutrients(rice.nutrients)
        self.assertTrue(result["complete"])
        self.assertEqual(
            {
                item["target_code"]: (item["fndds_nutrient_code"], item["source_nutrient_id"], item["source_unit"])
                for item in result["required_nutrients"]
            },
            {
                "energy_kcal": ("208", 1008, "kcal"),
                "protein_g": ("203", 1003, "g"),
                "carbohydrate_g": ("205", 1005, "g"),
                "fat_g": ("204", 1004, "g"),
            },
        )

        energy_rows = [
            item for item in rice.nutrients
            if item.get("nutrient", {}).get("number") == "208"
        ]
        general_energy = copy.deepcopy(energy_rows[0])
        general_energy["nutrient"]["id"] = 2047
        mapped_general_energy = summarize_fndds_required_nutrients(
            [item for item in rice.nutrients if item not in energy_rows] + [general_energy]
        )
        self.assertTrue(mapped_general_energy["complete"])
        self.assertEqual(
            next(
                item
                for item in mapped_general_energy["required_nutrients"]
                if item["target_code"] == "energy_kcal"
            )["source_nutrient_id"],
            2047,
        )

        specific_energy = copy.deepcopy(energy_rows[0])
        specific_energy["nutrient"]["id"] = 2048
        rejected = summarize_fndds_required_nutrients(
            [item for item in rice.nutrients if item not in energy_rows] + [specific_energy]
        )
        self.assertFalse(rejected["complete"])
        self.assertIn("unknown_source_nutrient_id", {item["reason_code"] for item in rejected["rejected"]})

        duplicate = summarize_fndds_required_nutrients(rice.nutrients + (energy_rows[0],))
        self.assertFalse(duplicate["complete"])
        self.assertIn("ambiguous_required_nutrient", {item["reason_code"] for item in duplicate["rejected"]})

    def test_one_exact_candidate_packet_is_deterministic_and_pending_review(self) -> None:
        first = build_fndds_review_report(self.parsed, self.policy, self.source_evidence)
        second = build_fndds_review_report(self.parsed, self.policy, self.source_evidence)
        self.assertEqual(
            json.dumps(first, ensure_ascii=False, sort_keys=True),
            json.dumps(second, ensure_ascii=False, sort_keys=True),
        )
        self.assertEqual(first["counts"]["target_count"], 7)
        self.assertEqual(first["counts"]["candidate_packet_count"], 1)
        self.assertFalse(first["production_eligible"])
        self.assertFalse(first["staged_candidate_created"])
        self.assertFalse(first["activation_attempted"])

        packet = first["candidate_packets"][0]
        self.assertEqual(packet["normalized_vietnamese_target"], "cơm trắng")
        self.assertEqual(packet["source_record"]["fndds_food_code"], "56205008")
        self.assertEqual(packet["source_record"]["fdc_id"], 2708408)
        self.assertEqual(packet["source_record"]["description"], "Rice, white, cooked, no added fat")
        self.assertEqual(
            packet["source_record"]["record_sha256"],
            "634cf6fabbcc9ccb76f513ef274a9dcc3d92f495c78792cf8221170d98a102cc",
        )
        self.assertEqual(packet["reviewer_state"], "pending_human_review")
        self.assertIsNone(packet["reviewer_reference"])
        self.assertFalse(packet["reviewer_approved"])
        self.assertTrue(packet["required_nutrient_completeness"]["complete"])
        self.assertEqual(packet["nutrient_mapping_version"], FNDDS_NUTRIENT_MAPPING_VERSION)
        self.assertEqual(len(packet["source_nutrient_definitions"]), 65)
        self.assertTrue(
            all(not item["source_method_available"] for item in packet["source_nutrient_definitions"])
        )

        serialized_packet = json.dumps(packet, ensure_ascii=False, sort_keys=True)
        for forbidden_field in ("foodPortions", "gramWeight", "portionDescription", '"amount"'):
            self.assertNotIn(forbidden_field, serialized_packet)
        self.assertEqual(
            {item["reason_code"] for item in packet["rejected_alternatives"]},
            {
                "fat_state_unspecified",
                "added_fat_variant",
                "glutinous_rice_variant_not_selected",
            },
        )
        self.assertEqual(
            first["foundation_precedence"]["primary_source"],
            "usda_fdc_foundation",
        )
        self.assertEqual(
            first["foundation_precedence"]["secondary_source"],
            "usda_fndds",
        )
        self.assertFalse(packet["source_precedence"]["foundation_accepted_mapping_exists"])

        egg = next(item for item in first["target_results"] if item["target_id"] == "vmb-public-0005")
        self.assertEqual(egg["outcome"], "no_proposal")
        self.assertEqual(egg["reason_code"], "preparation_ambiguous")

    def test_accepted_foundation_mapping_suppresses_secondary_candidate(self) -> None:
        report = build_fndds_review_report(
            self.parsed,
            self.policy,
            self.source_evidence,
            foundation_accepted_target_ids={"vmb-public-0002"},
        )
        self.assertEqual(report["counts"]["candidate_packet_count"], 0)
        result = next(item for item in report["target_results"] if item["target_id"] == "vmb-public-0002")
        self.assertEqual(result["outcome"], "suppressed_foundation_primary")

    def test_source_row_corruption_unknown_ids_and_composites_fail_closed(self) -> None:
        payload = json.loads(self.payload)
        payload["SurveyFoods"][0].pop("foodCode")
        corrupted_bytes = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        corrupted = FnddsSurveyAdapter().parse(corrupted_bytes)
        self.assertFalse(corrupted.valid)
        self.assertEqual(corrupted.rejected_records[0].reason_code, "invalid_food_code")
        with self.assertRaises(FnddsReviewError):
            build_fndds_review_report(corrupted, self.policy, self.source_evidence)

        composite_policy = copy.deepcopy(self.policy)
        rice = next(
            item for item in composite_policy["policy"]["targets"]
            if item["target_id"] == "vmb-public-0002"
        )
        rice["candidate"].update(
            {
                "fndds_food_code": "58150110",
                "fdc_id": 2708951,
                "description": "Rice, fried, meatless",
            }
        )
        with self.assertRaisesRegex(FnddsReviewError, "category"):
            build_fndds_review_report(self.parsed, composite_policy, self.source_evidence)

        near_policy = copy.deepcopy(self.policy)
        rice = next(
            item for item in near_policy["policy"]["targets"]
            if item["target_id"] == "vmb-public-0002"
        )
        rice["candidate"]["fndds_food_code"] = "56205001"
        with self.assertRaises(FnddsReviewError):
            build_fndds_review_report(self.parsed, near_policy, self.source_evidence)

    def test_incomplete_required_nutrient_profile_never_emits_candidate(self) -> None:
        policy = copy.deepcopy(self.policy)
        rice = next(item for item in policy["policy"]["targets"] if item["target_id"] == "vmb-public-0002")
        rice["candidate"].update(
            {
                "fndds_food_code": "11000000",
                "fdc_id": 2705383,
                "description": "Milk, human",
                "wweia_category_description": "Human milk",
            }
        )
        report = build_fndds_review_report(self.parsed, policy, self.source_evidence)
        self.assertEqual(report["counts"]["candidate_packet_count"], 0)
        result = next(item for item in report["target_results"] if item["target_id"] == "vmb-public-0002")
        self.assertEqual(result["outcome"], "no_proposal")
        self.assertEqual(result["reason_code"], "required_nutrients_incomplete")


if __name__ == "__main__":
    unittest.main()
