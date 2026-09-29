from __future__ import annotations

import copy
import hashlib
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "tests"))

from nutrition_data_factory.portion_planning import (  # noqa: E402
    PLANNING_MANIFEST_REF,
    REVIEWED_MAPPING_REF,
    REVIEWED_MAPPING_SHA256,
    build_portion_planning_packet,
    canonical_json_bytes,
)
from nutrition_data_factory.portion_study_definition import (  # noqa: E402
    OPTION_A_DECISION_REFERENCE,
    OPTION_A_DECISION_SHA256,
    OPTION_A_STRATEGY,
    PREVIOUS_MANIFEST_SHA256,
    PREVIOUS_PLANNING_PACKET_SHA256,
    STRATEGY_DECISION_REF,
    STRATEGY_DECISION_SCHEMA_VERSION,
    STUDY_DEFINITION_POLICY_VERSION,
    STUDY_DEFINITION_REF,
    STUDY_DEFINITION_SCHEMA_VERSION,
    STUDY_DESIGN_INPUTS,
    build_study_definition,
    is_study_definition_ready_for_measurement,
    option_a_decision_bytes,
    validate_option_a_decision,
)
from prepare_portion_study_definition import main as prepare_study_definition  # noqa: E402
from test_reviewed_mapping import _assert_json_schema_conforms  # noqa: E402


MAPPING_PATH = ROOT / "docs" / "reviews" / "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json"
PREVIOUS_MANIFEST_PATH = ROOT / "config" / "portion-study-com-trang-bat-0.3.0.json"
PREVIOUS_PACKET_PATH = ROOT / "docs" / "reviews" / "vietnamese-com-trang-bat-portion-planning-0.1.0.json"
DECISION_PATH = ROOT / "docs" / "reviews" / "vietnamese-com-trang-bat-study-strategy-decision-0.1.0.json"
DEFINITION_PATH = ROOT / "docs" / "reviews" / "vietnamese-com-trang-bat-study-definition-0.1.0.json"
EXPECTED_DEFINITION_SHA256 = "37458e9824377cb6e65605211f4995516adffae266824057534b6091bb9fbe49"


def build_committed_definition() -> tuple[dict[str, Any] | None, tuple[dict[str, str], ...]]:
    return build_study_definition(
        DECISION_PATH.read_bytes(),
        MAPPING_PATH.read_bytes(),
        PREVIOUS_MANIFEST_PATH.read_bytes(),
        PREVIOUS_PACKET_PATH.read_bytes(),
    )


class PortionStudyDefinitionTests(unittest.TestCase):
    def test_decision_artifact_pins_exact_option_a_owner_approval(self) -> None:
        decision_bytes = DECISION_PATH.read_bytes()
        decision = json.loads(decision_bytes)
        schema = json.loads(
            (ROOT / "schemas" / "portion-study-strategy-decision-0.1.0.schema.json").read_text(
                encoding="utf-8"
            )
        )

        self.assertEqual(decision_bytes, option_a_decision_bytes())
        self.assertEqual(hashlib.sha256(decision_bytes).hexdigest(), OPTION_A_DECISION_SHA256)
        self.assertEqual(decision["schema_version"], STRATEGY_DECISION_SCHEMA_VERSION)
        self.assertEqual(decision["issue_reference"], "github:issue/53")
        self.assertEqual(decision["target"], {"normalized_vietnamese_target": "cơm trắng", "canonical_measure": "bát"})
        self.assertEqual(decision["decision"], "approved")
        self.assertEqual(decision["strategy"], OPTION_A_STRATEGY)
        self.assertEqual(decision["reviewer"], "pumni")
        self.assertEqual(decision["decision_reference"], OPTION_A_DECISION_REFERENCE)
        self.assertFalse(decision["generalizable_to_all_vietnamese_bat"])
        self.assertEqual(decision["decision_scope"], "study_strategy_only")
        self.assertFalse(decision["physical_study_definition_approved"])
        self.assertFalse(decision["measurement_plan_values_approved"])
        self.assertFalse(decision["gram_value_approved"])
        self.assertTrue(decision["broader_household_study_requires_separate_decision"])
        self.assertEqual(decision["food_evidence"]["reviewed_mapping_ref"], REVIEWED_MAPPING_REF)
        self.assertEqual(decision["food_evidence"]["reviewed_mapping_sha256"], REVIEWED_MAPPING_SHA256)
        self.assertEqual(decision["food_evidence"]["source"]["fndds_food_code"], "56205008")
        self.assertEqual(decision["food_evidence"]["source"]["fdc_id"], 2708408)
        self.assertIsNone(decision["food_evidence"]["backend_food_id"])
        _assert_json_schema_conforms(decision, schema)

    def test_decision_mutations_fail_closed(self) -> None:
        original = json.loads(DECISION_PATH.read_text(encoding="utf-8"))
        schema = json.loads(
            (ROOT / "schemas" / "portion-study-strategy-decision-0.1.0.schema.json").read_text(
                encoding="utf-8"
            )
        )
        mutations = (
            ("strategy", lambda value: value.__setitem__("strategy", "household_use_sampling_frame")),
            ("reviewer", lambda value: value.__setitem__("reviewer", "assistant-bot")),
            (
                "decision reference",
                lambda value: value.__setitem__("decision_reference", "github:issue/53#wrong"),
            ),
            (
                "generalization boundary",
                lambda value: value.__setitem__("generalizable_to_all_vietnamese_bat", True),
            ),
            (
                "reviewed mapping hash",
                lambda value: value["food_evidence"].__setitem__("reviewed_mapping_sha256", "0" * 64),
            ),
            (
                "source identity",
                lambda value: value["food_evidence"]["source"].__setitem__(
                    "fdc_id", 2708409
                ),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(label=label):
                changed = copy.deepcopy(original)
                mutate(changed)
                validated, errors = validate_option_a_decision(changed)
                self.assertIsNone(validated)
                self.assertTrue(errors)
                with self.assertRaises(AssertionError):
                    _assert_json_schema_conforms(changed, schema)

    def test_study_shell_is_deterministic_versioned_and_schema_conformant(self) -> None:
        definition, errors = build_committed_definition()
        self.assertEqual(errors, ())
        self.assertIsNotNone(definition)
        definition_bytes = canonical_json_bytes(definition)
        definition_schema = json.loads(
            (ROOT / "schemas" / "portion-study-definition-0.1.0.schema.json").read_text(
                encoding="utf-8"
            )
        )
        committed = json.loads(DEFINITION_PATH.read_text(encoding="utf-8"))
        _assert_json_schema_conforms(committed, definition_schema)
        self.assertEqual(committed, definition)
        self.assertEqual(committed["schema_version"], STUDY_DEFINITION_SCHEMA_VERSION)
        self.assertEqual(committed["policy_version"], STUDY_DEFINITION_POLICY_VERSION)
        self.assertEqual(hashlib.sha256(definition_bytes).hexdigest(), EXPECTED_DEFINITION_SHA256)
        self.assertEqual(committed["strategy_decision"]["artifact_ref"], STRATEGY_DECISION_REF)
        self.assertEqual(committed["strategy_decision"]["sha256"], OPTION_A_DECISION_SHA256)
        self.assertFalse(committed["strategy_decision"]["physical_study_definition_approved"])
        self.assertFalse(committed["strategy_decision"]["measurement_plan_values_approved"])
        self.assertEqual(
            committed["predecessor"]["manifest"]["sha256"], PREVIOUS_MANIFEST_SHA256
        )
        self.assertEqual(
            committed["predecessor"]["planning_packet"]["sha256"],
            PREVIOUS_PLANNING_PACKET_SHA256,
        )
        self.assertEqual(committed["target"]["reviewed_evidence_identity"]["sha256"], REVIEWED_MAPPING_SHA256)
        self.assertIsNone(committed["target"]["backend_food_id"])
        self.assertEqual(committed["measurement_protocol"]["protocol_version"], "portion-measurement-0.1.0")
        self.assertEqual(set(committed["study_design_inputs"]), set(STUDY_DESIGN_INPUTS))
        for item in committed["study_design_inputs"].values():
            self.assertEqual(item, {"status": "unresolved", "value": None, "reviewer": None, "review_ref": None})
        self.assertEqual(committed["future_observations"], {"status": "not_collected", "observation_count": 0, "records": []})
        self.assertIsNone(committed["estimates"]["mass_estimate"])
        self.assertFalse(committed["estimates"]["gram_estimate_emitted"])
        self.assertFalse(committed["readiness"]["ready_for_measurement"])
        self.assertFalse(committed["readiness"]["review_ready"])
        self.assertFalse(committed["publication"]["publishable"])
        self.assertFalse(committed["publication"]["production_eligible"])
        self.assertFalse(committed["publication"]["release_created"])
        self.assertFalse(committed["publication"]["activation_attempted"])
        serialized = DEFINITION_PATH.read_text(encoding="utf-8")
        self.assertNotIn("mass_g", serialized)
        self.assertNotIn("fixture", serialized.lower())
        self.assertEqual(DEFINITION_PATH.read_bytes(), definition_bytes)

    def test_approved_strategy_with_each_missing_design_group_stays_not_ready(self) -> None:
        definition, errors = build_committed_definition()
        self.assertEqual(errors, ())
        self.assertEqual(definition["strategy_decision"]["decision"], "approved")
        self.assertFalse(is_study_definition_ready_for_measurement(definition))
        required_missing_groups = (
            ("vessel definition", ("vessel_definition",)),
            ("serving protocol", ("serving_fill_protocol",)),
            (
                "sample and batch plan",
                ("minimum_independent_sample_count", "minimum_independent_cooking_batch_count"),
            ),
            (
                "instrument, calibration, and tare",
                ("instrument_requirements", "calibration_check_requirements", "tare_method"),
            ),
            ("measurement reviewer", ("measurement_reviewer",)),
        )
        schema = json.loads(
            (ROOT / "schemas" / "portion-study-definition-0.1.0.schema.json").read_text(
                encoding="utf-8"
            )
        )
        for label, keys in required_missing_groups:
            with self.subTest(label=label):
                changed = copy.deepcopy(definition)
                for key in keys:
                    del changed["study_design_inputs"][key]
                self.assertFalse(is_study_definition_ready_for_measurement(changed))
                with self.assertRaises(AssertionError):
                    _assert_json_schema_conforms(changed, schema)

    def test_unreviewed_values_and_missing_review_references_never_enable_readiness(self) -> None:
        definition, errors = build_committed_definition()
        self.assertEqual(errors, ())
        changed = copy.deepcopy(definition)
        for item in changed["study_design_inputs"].values():
            item.update(
                {
                    "status": "reviewed",
                    "value": "unreviewed value",
                    "reviewer": "pumni",
                    "review_ref": None,
                }
            )
        self.assertFalse(is_study_definition_ready_for_measurement(changed))
        changed["readiness"]["ready_for_measurement"] = True
        self.assertFalse(is_study_definition_ready_for_measurement(changed))
        schema = json.loads(
            (ROOT / "schemas" / "portion-study-definition-0.1.0.schema.json").read_text(
                encoding="utf-8"
            )
        )
        with self.assertRaises(AssertionError):
            _assert_json_schema_conforms(changed, schema)

    def test_pr52_planning_artifacts_remain_byte_identical_and_have_no_strategy_decision(self) -> None:
        manifest_bytes = PREVIOUS_MANIFEST_PATH.read_bytes()
        packet_bytes = PREVIOUS_PACKET_PATH.read_bytes()
        self.assertEqual(hashlib.sha256(manifest_bytes).hexdigest(), PREVIOUS_MANIFEST_SHA256)
        self.assertEqual(hashlib.sha256(packet_bytes).hexdigest(), PREVIOUS_PLANNING_PACKET_SHA256)
        regenerated, errors = build_portion_planning_packet(manifest_bytes, MAPPING_PATH.read_bytes())
        self.assertEqual(errors, ())
        self.assertEqual(packet_bytes, canonical_json_bytes(regenerated))
        self.assertNotIn("strategy_decision", regenerated)
        self.assertNotIn(OPTION_A_DECISION_REFERENCE, packet_bytes.decode("utf-8"))

    def test_builder_rejects_changed_decision_mapping_and_predecessor_evidence(self) -> None:
        decision_bytes = DECISION_PATH.read_bytes()
        mapping_bytes = MAPPING_PATH.read_bytes()
        manifest_bytes = PREVIOUS_MANIFEST_PATH.read_bytes()
        packet_bytes = PREVIOUS_PACKET_PATH.read_bytes()
        changed_inputs = (
            ("decision", decision_bytes + b" ", mapping_bytes, manifest_bytes, packet_bytes),
            ("mapping", decision_bytes, mapping_bytes + b" ", manifest_bytes, packet_bytes),
            ("previous manifest", decision_bytes, mapping_bytes, manifest_bytes + b" ", packet_bytes),
            ("previous planning packet", decision_bytes, mapping_bytes, manifest_bytes, packet_bytes + b" "),
        )
        for label, changed_decision, changed_mapping, changed_manifest, changed_packet in changed_inputs:
            with self.subTest(label=label):
                result, errors = build_study_definition(
                    changed_decision, changed_mapping, changed_manifest, changed_packet
                )
                self.assertIsNone(result)
                self.assertTrue(errors)

    def test_materializer_is_deterministic_and_refuses_conflicting_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            decision_output = Path(directory) / "decision.json"
            definition_output = Path(directory) / "definition.json"
            args = [
                "--decision-output",
                str(decision_output),
                "--definition-output",
                str(definition_output),
            ]
            with redirect_stdout(StringIO()):
                self.assertEqual(prepare_study_definition(args), 0)
                first_decision = decision_output.read_bytes()
                first_definition = definition_output.read_bytes()
                self.assertEqual(prepare_study_definition(args), 0)
            self.assertEqual(first_decision, option_a_decision_bytes())
            self.assertEqual(definition_output.read_bytes(), first_definition)

            decision_output.write_bytes(b"conflicting decision")
            with redirect_stdout(StringIO()), self.assertRaisesRegex(RuntimeError, "refusing to overwrite"):
                prepare_study_definition(args)
