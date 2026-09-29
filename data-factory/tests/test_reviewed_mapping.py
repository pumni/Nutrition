from __future__ import annotations

import copy
import hashlib
import json
import re
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
DECISION_SCHEMA_PATH = ROOT / "schemas" / "curation-decision-0.2.0.json"
MAPPING_SCHEMA_PATH = ROOT / "schemas" / "reviewed-food-mapping-0.1.0.schema.json"
EXPECTED_MAPPING_SHA256 = "c7ab626ed66c01a84c6b00d7e312a3e061eef4af2ec4dcfed950e6b398307324"
EXPECTED_DECISION_SHA256 = "c6404b8cbfd3360da83798088bc7d7c34d5c91daa94da2f357ff1bba0e959c12"
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

    def test_unrelated_current_registry_change_preserves_materialized_bytes(self) -> None:
        registry = copy.deepcopy(json.loads(self.registry_bytes))
        unrelated = next(item for item in registry["sources"] if item["code"] == "usda_fdc_foundation")
        unrelated["purpose"] = "Updated unrelated source metadata"
        changed_registry_bytes = (
            json.dumps(registry, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
        ).encode("utf-8")

        changed_decision, changed_mapping = materialize_reviewed_rice_mapping(
            self.report_bytes, changed_registry_bytes
        )

        self.assertEqual(changed_decision, self.decision_bytes)
        self.assertEqual(changed_mapping, self.mapping_bytes)
        self.assertEqual(sha256(changed_mapping), EXPECTED_MAPPING_SHA256)

    def test_current_fndds_rights_and_staging_boundary_changes_fail_closed(self) -> None:
        mutations = (
            lambda source: source.__setitem__("rights_state", "approved"),
            lambda source: source.__setitem__("production_ingestion", "allowed"),
            lambda source: source["allowed_uses"].append("staged_candidate"),
            lambda source: source["prohibited_uses"].remove("staged_candidate"),
            lambda source: source.__setitem__("production_eligible", True),
        )
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                registry = copy.deepcopy(json.loads(self.registry_bytes))
                source = next(item for item in registry["sources"] if item["code"] == "usda_fndds")
                mutate(source)
                changed_registry_bytes = (
                    json.dumps(registry, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
                ).encode("utf-8")
                with self.assertRaisesRegex(ReviewedMappingError, "source-wide non-staging"):
                    materialize_reviewed_rice_mapping(self.report_bytes, changed_registry_bytes)

    def test_committed_decision_and_mapping_conform_to_their_json_schemas(self) -> None:
        decision_schema = json.loads(DECISION_SCHEMA_PATH.read_text(encoding="utf-8"))
        mapping_schema = json.loads(MAPPING_SCHEMA_PATH.read_text(encoding="utf-8"))
        committed_decision = json.loads(DECISION_PATH.read_text(encoding="utf-8"))
        committed_mapping = json.loads(MAPPING_PATH.read_text(encoding="utf-8"))

        _assert_json_schema_conforms(committed_decision, decision_schema)
        _assert_json_schema_conforms(committed_mapping, mapping_schema)

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
        self.assertEqual(report["source_registry_evidence"]["sha256"], SOURCE_REGISTRY_SHA256)
        self.assertEqual(packet["reviewer_state"], "pending_human_review")
        self.assertIsNone(packet["reviewer_reference"])
        self.assertFalse(packet["reviewer_approved"])

    def test_fndds_source_registry_remains_source_wide_non_staging(self) -> None:
        registry = json.loads(self.registry_bytes)
        source = next(item for item in registry["sources"] if item["code"] == "usda_fndds")
        self.assertEqual(source["rights_state"], "reference_only")
        self.assertEqual(source["production_ingestion"], "blocked_until_product_selection")
        self.assertEqual(source["allowed_uses"], ["analysis", "reference"])
        self.assertIn("staged_candidate", source["prohibited_uses"])
        self.assertFalse(source["production_eligible"])
        boundary = self.mapping["source_registry_boundary"]
        self.assertEqual(boundary["historical_report_registry_sha256"], SOURCE_REGISTRY_SHA256)
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


_SUPPORTED_SCHEMA_KEYWORDS = {
    "$id",
    "$schema",
    "$defs",
    "$ref",
    "additionalProperties",
    "allOf",
    "const",
    "enum",
    "if",
    "items",
    "maxItems",
    "minItems",
    "minLength",
    "oneOf",
    "pattern",
    "properties",
    "required",
    "then",
    "type",
}


def _assert_json_schema_conforms(
    instance: object,
    schema: dict[str, object],
    path: str = "$",
    root_schema: dict[str, object] | None = None,
) -> None:
    if root_schema is None:
        root_schema = schema
    unsupported = set(schema) - _SUPPORTED_SCHEMA_KEYWORDS
    assert not unsupported, f"{path}: unsupported schema keywords {sorted(unsupported)}"

    if "$ref" in schema:
        reference = schema["$ref"]
        assert isinstance(reference, str) and reference.startswith("#/$defs/"), (
            f"{path}: unsupported schema reference {reference}"
        )
        definition_name = reference.removeprefix("#/$defs/")
        definitions = root_schema.get("$defs", {})
        assert isinstance(definitions, dict) and definition_name in definitions, (
            f"{path}: unresolved schema reference {reference}"
        )
        referenced_schema = definitions[definition_name]
        assert isinstance(referenced_schema, dict)
        _assert_json_schema_conforms(instance, referenced_schema, path, root_schema)
        return

    expected_type = schema.get("type")
    if expected_type is not None:
        allowed_types = expected_type if isinstance(expected_type, list) else [expected_type]
        assert any(_matches_json_type(instance, item) for item in allowed_types), (
            f"{path}: expected JSON type {expected_type}"
        )
    if "const" in schema:
        expected = schema["const"]
        assert type(instance) is type(expected) and instance == expected, f"{path}: const mismatch"
    if "enum" in schema:
        assert any(type(instance) is type(item) and instance == item for item in schema["enum"]), (
            f"{path}: value is outside enum"
        )
    if isinstance(instance, str):
        if "minLength" in schema:
            assert len(instance) >= schema["minLength"], f"{path}: string is too short"
        if "pattern" in schema:
            assert re.search(schema["pattern"], instance) is not None, f"{path}: pattern mismatch"

    if isinstance(instance, dict):
        required = schema.get("required", [])
        assert all(key in instance for key in required), f"{path}: required property missing"
        properties = schema.get("properties", {})
        assert isinstance(properties, dict)
        additional = schema.get("additionalProperties", True)
        for key, value in instance.items():
            child_path = f"{path}.{key}"
            if key in properties:
                _assert_json_schema_conforms(value, properties[key], child_path, root_schema)
            elif additional is False:
                raise AssertionError(f"{child_path}: additional property is forbidden")
            elif isinstance(additional, dict):
                _assert_json_schema_conforms(value, additional, child_path, root_schema)

    if isinstance(instance, list):
        if "minItems" in schema:
            assert len(instance) >= schema["minItems"], f"{path}: too few items"
        if "maxItems" in schema:
            assert len(instance) <= schema["maxItems"], f"{path}: too many items"
        if "items" in schema:
            for index, value in enumerate(instance):
                _assert_json_schema_conforms(value, schema["items"], f"{path}[{index}]", root_schema)

    for child_schema in schema.get("allOf", []):
        _assert_json_schema_conforms(instance, child_schema, path, root_schema)
    if "oneOf" in schema:
        valid_schemas = 0
        for child_schema in schema["oneOf"]:
            try:
                _assert_json_schema_conforms(instance, child_schema, path, root_schema)
            except AssertionError:
                continue
            valid_schemas += 1
        assert valid_schemas == 1, f"{path}: expected exactly one matching schema"
    if "if" in schema:
        try:
            _assert_json_schema_conforms(instance, schema["if"], path, root_schema)
        except AssertionError:
            pass
        else:
            if "then" in schema:
                _assert_json_schema_conforms(instance, schema["then"], path, root_schema)


def _matches_json_type(instance: object, expected: object) -> bool:
    if expected == "null":
        return instance is None
    if expected == "object":
        return isinstance(instance, dict)
    if expected == "array":
        return isinstance(instance, list)
    if expected == "string":
        return isinstance(instance, str)
    if expected == "boolean":
        return isinstance(instance, bool)
    if expected == "integer":
        return type(instance) is int
    if expected == "number":
        return type(instance) in (int, float)
    raise AssertionError(f"unsupported JSON type: {expected}")


if __name__ == "__main__":
    unittest.main()
