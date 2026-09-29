from __future__ import annotations

import copy
import hashlib
import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.curation.packets import (  # noqa: E402
    CURATION_DECISION_POLICY_VERSION,
    CurationDecision,
    DecisionHistory,
    SourceRecordIdentity,
    validate_decision,
)
from nutrition_data_factory.curation.reviewed_mapping import (  # noqa: E402
    DECISION_REFERENCE,
    PROPOSAL_ID,
    REPORT_SHA256,
    REVIEWED_AT,
    REVIEWER,
    SOURCE_RECORD,
    SOURCE_REGISTRY_SHA256,
    SOURCE_STRATEGY_REFERENCE,
    ReviewedMappingError,
    materialize_reviewed_rice_mapping,
    sha256,
)


REPORT_PATH = ROOT / "docs" / "reviews" / "fndds-secondary-source-issue-44.json"
HISTORICAL_REPORT_MARKDOWN_PATH = ROOT / "docs" / "reviews" / "fndds-secondary-source-issue-44.md"
REGISTRY_PATH = ROOT / "config" / "source_registry.json"
DECISION_PATH = ROOT / "docs" / "reviews" / "vietnamese-basic-food-identity-issue-35-decisions-0.2.0.jsonl"
MAPPING_PATH = ROOT / "docs" / "reviews" / "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json"
EXPECTED_MAPPING_SHA256 = "3e93c7534e0f2733ca3e4d031ae590abeb5b4c3dad3d9c398fd496d6343c691a"
EXPECTED_DECISION_SHA256 = "962aef61ffa2c57b34250630c254c5453b8a533324da288e3503c1ad72e5b2b8"
EXPECTED_HISTORICAL_REPORT_MARKDOWN_SHA256 = "b743145ee8ccf7b9c7a74007de9418bc7bc0f5da7d5532dd34a85fbc675ba512"


class ReviewedRiceMappingTests(unittest.TestCase):
    def setUp(self) -> None:
        self.report_bytes = REPORT_PATH.read_bytes()
        self.registry_bytes = REGISTRY_PATH.read_bytes()
        self.decision_bytes, self.mapping_bytes = materialize_reviewed_rice_mapping(
            self.report_bytes, self.registry_bytes
        )
        self.decision = json.loads(self.decision_bytes.decode("utf-8"))
        self.mapping = json.loads(self.mapping_bytes)

    def test_exact_human_decision_metadata_and_structured_source_identity_are_pinned(self) -> None:
        self.assertEqual(CURATION_DECISION_POLICY_VERSION, "curation-decision-0.2.0")
        self.assertEqual(self.decision["candidate_id"], PROPOSAL_ID)
        self.assertEqual(self.decision["decision_type"], "food_source_mapping")
        self.assertEqual(self.decision["decision"], "approved")
        self.assertEqual(self.decision["decision_reference"], DECISION_REFERENCE)
        self.assertEqual(self.decision["reviewer"], REVIEWER)
        self.assertEqual(self.decision["reviewed_at"], REVIEWED_AT)
        self.assertEqual(self.decision["policy_version"], "curation-decision-0.2.0")
        self.assertIn("Cultivar distinction is not required", self.decision["rationale"])
        self.assertEqual(
            self.decision["source_record_identity"],
            {
                "source_code": "usda_fndds",
                "release": "2021-2023 / October 2024",
                "source_record_id": "56205008",
                "external_id_type": "fdc_id",
                "external_record_id": "2708408",
                "description": "Rice, white, cooked, no added fat",
                "record_sha256": SOURCE_RECORD["record_sha256"],
                "archive_sha256": SOURCE_RECORD["archive_sha256"],
                "archive_member": SOURCE_RECORD["archive_member"],
                "member_sha256": SOURCE_RECORD["member_sha256"],
                "schema_fingerprint": SOURCE_RECORD["schema_fingerprint"],
            },
        )
        self.assertIn(SOURCE_STRATEGY_REFERENCE, self.decision["evidence_refs"])

    def test_human_reviewer_validation_remains_enforced_for_source_mapping(self) -> None:
        identity = SourceRecordIdentity(**self.decision["source_record_identity"])
        decision = CurationDecision(
            decision_id=self.decision["decision_id"],
            decision_type=self.decision["decision_type"],
            candidate_id=self.decision["candidate_id"],
            decision=self.decision["decision"],
            reviewer=self.decision["reviewer"],
            reviewed_at=self.decision["reviewed_at"],
            rationale=self.decision["rationale"],
            evidence_refs=tuple(self.decision["evidence_refs"]),
            policy_version=self.decision["policy_version"],
            decision_reference=self.decision["decision_reference"],
            source_record_identity=identity,
        )
        validate_decision(decision)
        history = DecisionHistory()
        with self.assertRaisesRegex(ValueError, "machine identities"):
            history.append(CurationDecision(**{**decision.__dict__, "reviewer": "assistant-bot"}))

    def test_pinned_report_and_exact_source_identifiers_reject_mutation(self) -> None:
        mutations = (
            (lambda report: report["candidate_packets"][0].__setitem__("proposal_id", "other-proposal"), "proposal ID"),
            (lambda report: report["candidate_packets"][0]["source_record"].__setitem__("fndds_food_code", "56205001"), "FNDDS/FDC source record"),
            (lambda report: report["candidate_packets"][0]["source_record"].__setitem__("fdc_id", 2708403), "FNDDS/FDC source record"),
            (lambda report: report["candidate_packets"][0]["source_record"].__setitem__("record_sha256", "0" * 64), "source record identity or SHA-256"),
            (lambda report: report["source_artifact"].__setitem__("archive_sha256", "0" * 64), "archive/member/schema"),
            (lambda report: report["source_artifact"].__setitem__("member_sha256", "0" * 64), "archive/member/schema"),
            (lambda report: report["source_artifact"].__setitem__("schema_fingerprint", "0" * 64), "archive/member/schema"),
        )
        for mutate, message in mutations:
            with self.subTest(message=message):
                report = copy.deepcopy(json.loads(self.report_bytes))
                mutate(report)
                changed_bytes = json.dumps(report, ensure_ascii=False, sort_keys=True).encode("utf-8")
                with self.assertRaisesRegex(ReviewedMappingError, message):
                    materialize_reviewed_rice_mapping(changed_bytes, self.registry_bytes)

    def test_reviewed_artifact_is_deterministic_and_hash_is_stable(self) -> None:
        repeated_decision, repeated_mapping = materialize_reviewed_rice_mapping(
            self.report_bytes, self.registry_bytes
        )
        self.assertEqual(self.decision_bytes, repeated_decision)
        self.assertEqual(self.mapping_bytes, repeated_mapping)
        self.assertEqual(sha256(self.mapping_bytes), EXPECTED_MAPPING_SHA256)
        self.assertEqual(sha256(self.decision_bytes), EXPECTED_DECISION_SHA256)
        self.assertEqual(MAPPING_PATH.read_bytes(), self.mapping_bytes)
        self.assertEqual(DECISION_PATH.read_bytes(), self.decision_bytes)

    def test_generic_mapping_boundary_excludes_more_specific_rice_identities(self) -> None:
        mapping = self.mapping["mapping"]
        self.assertEqual(mapping["normalized_vietnamese_target"], "cơm trắng")
        self.assertEqual(mapping["identity_status"], "reviewed_supported")
        self.assertEqual(mapping["composition_status"], "reviewed_supported")
        self.assertEqual(mapping["runtime_specificity_policy_reference"], "resolve-exact-specificity-0.2.0")
        rejected = set(mapping["not_approved_specificity"])
        self.assertTrue(any("jasmine" in item for item in rejected))
        self.assertTrue(any("ST25" in item for item in rejected))
        self.assertTrue(any("nếp" in item and "glutinous" in item for item in rejected))
        self.assertTrue(any("gạo lứt" in item and "brown rice" in item for item in rejected))
        self.assertTrue(any("rang" in item and "chiên" in item and "fried" in item for item in rejected))
        self.assertTrue(any("added oil" in item or "added fat" in item for item in rejected))
        self.assertTrue(any("materially different preparation" in item for item in rejected))

    def test_coverage_keeps_other_targets_unsupported_and_portion_recipe_separate(self) -> None:
        coverage = self.mapping["coverage_update"]
        others = coverage["other_bounded_fndds_targets"]
        self.assertEqual(len(others), 6)
        self.assertEqual({item["fndds_outcome"] for item in others}, {"no_proposal"})
        self.assertEqual({item["identity_status"] for item in others}, {"unsupported"})
        self.assertEqual({item["composition_status"] for item in others}, {"unsupported"})
        self.assertEqual(
            {item["normalized_vietnamese_target"] for item in others},
            {"trứng gà luộc", "thịt bò", "thịt gà", "thịt gà luộc", "sữa tươi", "rau muống"},
        )
        self.assertEqual(
            {item["normalized_vietnamese_target"] for item in coverage["composite_dishes"]},
            {"phở bò", "bún bò Huế", "cơm gà"},
        )
        self.assertTrue(all(item["status"] == "recipe_required" for item in coverage["composite_dishes"]))
        self.assertFalse(coverage["synthetic_fixture_bat_grams_promoted"])

        mapping = self.mapping["mapping"]
        self.assertEqual(
            mapping["portion_boundary"]["status"],
            "unsupported_without_explicit_grams_or_later_reviewed_portion_evidence",
        )
        self.assertFalse(mapping["portion_boundary"]["unit_portion_inference_approved"])
        self.assertFalse(mapping["portion_boundary"]["gram_weights_emitted"])
        self.assertFalse(
            any(type(value) in (int, float) for value in mapping["portion_boundary"].values())
        )
        self.assertFalse(mapping["recipe_boundary"]["recipe_evidence_emitted"])
        self.assertFalse(mapping["production_eligible"])
        self.assertFalse(mapping["activation_authorized"])

    def test_historical_issue_44_artifacts_remain_byte_identical_and_pending(self) -> None:
        self.assertEqual(hashlib.sha256(self.report_bytes).hexdigest(), REPORT_SHA256)
        self.assertEqual(
            hashlib.sha256(HISTORICAL_REPORT_MARKDOWN_PATH.read_bytes()).hexdigest(),
            EXPECTED_HISTORICAL_REPORT_MARKDOWN_SHA256,
        )
        report = json.loads(self.report_bytes)
        packet = report["candidate_packets"][0]
        self.assertEqual(packet["reviewer_state"], "pending_human_review")
        self.assertIsNone(packet["reviewer_reference"])
        self.assertFalse(packet["reviewer_approved"])

    def test_fndds_source_registry_remains_source_wide_non_staging(self) -> None:
        registry = json.loads(self.registry_bytes)
        source = next(item for item in registry["sources"] if item["code"] == "usda_fndds")
        self.assertEqual(sha256(self.registry_bytes), SOURCE_REGISTRY_SHA256)
        self.assertEqual(source["rights_state"], "reference_only")
        self.assertEqual(source["production_ingestion"], "blocked_until_product_selection")
        self.assertEqual(source["allowed_uses"], ["analysis", "reference"])
        self.assertIn("staged_candidate", source["prohibited_uses"])
        self.assertFalse(source["production_eligible"])
        boundary = self.mapping["source_registry_boundary"]
        self.assertFalse(boundary["source_wide_staged_candidate_permission"])
        self.assertFalse(boundary["production_eligible"])

    def test_curation_policy_requires_human_reference_and_structured_record_for_mapping(self) -> None:
        identity = SourceRecordIdentity(**self.decision["source_record_identity"])
        decision_data = {
            "decision_id": self.decision["decision_id"],
            "decision_type": "food_source_mapping",
            "candidate_id": PROPOSAL_ID,
            "decision": "approved",
            "reviewer": REVIEWER,
            "reviewed_at": REVIEWED_AT,
            "rationale": self.decision["rationale"],
            "evidence_refs": tuple(self.decision["evidence_refs"]),
            "policy_version": CURATION_DECISION_POLICY_VERSION,
            "decision_reference": DECISION_REFERENCE,
            "source_record_identity": identity,
        }
        with self.assertRaisesRegex(ValueError, "decision reference"):
            validate_decision(CurationDecision(**{**decision_data, "decision_reference": None}))
        with self.assertRaisesRegex(ValueError, "structured source record"):
            validate_decision(CurationDecision(**{**decision_data, "source_record_identity": None}))
        with self.assertRaisesRegex(ValueError, "requires curation-decision-0.2.0"):
            validate_decision(
                CurationDecision(**{**decision_data, "policy_version": "curation-decision-0.1.0"})
            )


if __name__ == "__main__":
    unittest.main()
