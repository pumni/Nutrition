from __future__ import annotations

import copy
import hashlib
import json
import re
import sys
import unittest
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "scripts"))

from nutrition_data_factory.portion_planning import (  # noqa: E402
    MANIFEST_POLICY_VERSION,
    MANIFEST_SCHEMA_VERSION,
    PLANNING_PACKET_POLICY_VERSION,
    PLANNING_PACKET_SCHEMA_VERSION,
    REVIEWED_MAPPING_SHA256,
    UNRESOLVED_HUMAN_INPUTS,
    build_portion_planning_packet,
    canonical_json_bytes,
    validate_portion_planning_manifest,
)
from nutrition_data_factory.portion import validate_manifest as validate_legacy_manifest  # noqa: E402
from prepare_portion_planning_packet import main as prepare_planning_packet  # noqa: E402


MANIFEST_PATH = ROOT / "config" / "portion-study-com-trang-bat-0.3.0.json"
MAPPING_PATH = (
    ROOT
    / "docs"
    / "reviews"
    / "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json"
)
PACKET_PATH = ROOT / "docs" / "reviews" / "vietnamese-com-trang-bat-portion-planning-0.1.0.json"


def manifest_value() -> dict[str, Any]:
    return json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))


def errors_contain(errors: tuple[dict[str, str], ...], reason_code: str) -> bool:
    return any(error.get("reason_code") == reason_code for error in errors)


class PortionPlanningTests(unittest.TestCase):
    def test_manifest_binds_exact_reviewed_evidence_without_backend_food_id(self) -> None:
        manifest = manifest_value()
        mapping_bytes = MAPPING_PATH.read_bytes()
        validated, errors = validate_portion_planning_manifest(manifest, mapping_bytes)

        self.assertIsNotNone(validated)
        self.assertEqual(errors, ())
        self.assertEqual(manifest["schema_version"], MANIFEST_SCHEMA_VERSION)
        self.assertEqual(manifest["policy_version"], MANIFEST_POLICY_VERSION)
        identity = manifest["target"]["identity"]
        self.assertEqual(identity["status"], "reviewed_evidence")
        self.assertIsNone(identity["backend_food_id"])
        self.assertEqual(
            identity["reviewed_evidence_identity"]["sha256"], REVIEWED_MAPPING_SHA256
        )
        self.assertEqual(
            identity["reviewed_evidence_identity"]["source"]["source_code"], "usda_fndds"
        )
        self.assertEqual(
            identity["reviewed_evidence_identity"]["source"]["fndds_food_code"], "56205008"
        )
        self.assertEqual(identity["reviewed_evidence_identity"]["source"]["fdc_id"], 2708408)
        self.assertEqual(
            identity["reviewed_evidence_identity"]["source"]["source_record_sha256"],
            "634cf6fabbcc9ccb76f513ef274a9dcc3d92f495c78792cf8221170d98a102cc",
        )
        self.assertEqual(
            identity["reviewed_evidence_identity"]["specificity_policy_ref"],
            "resolve-exact-specificity-0.2.0",
        )

    def test_changed_mapping_hash_or_source_identity_fails_closed(self) -> None:
        mapping_bytes = MAPPING_PATH.read_bytes()
        manifest = manifest_value()

        wrong_hash = copy.deepcopy(manifest)
        wrong_hash["target"]["identity"]["reviewed_evidence_identity"]["sha256"] = "0" * 64
        _, errors = validate_portion_planning_manifest(wrong_hash, mapping_bytes)
        self.assertTrue(errors_contain(errors, "reviewed_evidence_identity_mismatch"))

        wrong_source = copy.deepcopy(manifest)
        wrong_source["target"]["identity"]["reviewed_evidence_identity"]["source"][
            "fndds_food_code"
        ] = "00000000"
        _, errors = validate_portion_planning_manifest(wrong_source, mapping_bytes)
        self.assertTrue(errors_contain(errors, "reviewed_evidence_identity_mismatch"))

        changed_mapping = mapping_bytes.replace(b"2708408", b"2708409", 1)
        _, errors = validate_portion_planning_manifest(manifest, changed_mapping)
        self.assertTrue(errors_contain(errors, "reviewed_mapping_hash_mismatch"))
        self.assertTrue(errors_contain(errors, "reviewed_mapping_identity_mismatch"))

    def test_external_ids_cannot_masquerade_as_backend_food_ids(self) -> None:
        for external_id in ("56205008", 2708408, "fndds-proposal-d7772be2123024935757"):
            with self.subTest(external_id=external_id):
                manifest = copy.deepcopy(manifest_value())
                manifest["target"]["identity"]["backend_food_id"] = external_id
                _, errors = validate_portion_planning_manifest(manifest, MAPPING_PATH.read_bytes())
                self.assertTrue(
                    errors_contain(errors, "backend_food_id_not_assigned_for_reviewed_evidence")
                )

    def test_successor_manifest_is_not_reinterpreted_as_legacy_input(self) -> None:
        manifest = manifest_value()
        legacy, errors = validate_legacy_manifest(manifest)
        self.assertIsNone(legacy)
        self.assertTrue(errors_contain(errors, "unsupported_version"))
        self.assertTrue(any(error.get("field") == "schema_version" for error in errors))

    def test_planning_packet_is_deterministic_and_has_no_mass_estimate(self) -> None:
        manifest_bytes = MANIFEST_PATH.read_bytes()
        mapping_bytes = MAPPING_PATH.read_bytes()
        packet, errors = build_portion_planning_packet(manifest_bytes, mapping_bytes)
        replay, replay_errors = build_portion_planning_packet(manifest_bytes, mapping_bytes)

        self.assertEqual(errors, ())
        self.assertEqual(replay_errors, ())
        self.assertIsNotNone(packet)
        self.assertEqual(canonical_json_bytes(packet), canonical_json_bytes(replay))
        self.assertEqual(packet["schema_version"], PLANNING_PACKET_SCHEMA_VERSION)
        self.assertEqual(packet["policy_version"], PLANNING_PACKET_POLICY_VERSION)
        self.assertEqual(packet["status"], "blocked_unresolved_human_inputs")
        self.assertEqual(len(packet["unresolved_human_inputs"]), len(UNRESOLVED_HUMAN_INPUTS))
        self.assertEqual(packet["readiness"]["blocker_count"], len(UNRESOLVED_HUMAN_INPUTS))
        self.assertFalse(packet["readiness"]["ready_for_measurement"])
        self.assertFalse(packet["readiness"]["review_ready"])
        self.assertEqual(packet["measurement_state"], {"status": "not_collected", "observation_count": 0})
        self.assertIsNone(packet["mass_estimate"])
        self.assertFalse(packet["gram_estimate_emitted"])
        self.assertFalse(packet["publication"]["publishable"])
        self.assertFalse(packet["publication"]["release_created"])
        self.assertFalse(packet["publication"]["activation_attempted"])

        descriptions = [item["description"] for item in packet["unresolved_human_inputs"]]
        for expected in (
            'Physical definition/context of "bát"',
            "Vessel/context sampling strategy",
            "Minimum independent sample count",
            "Minimum batch count",
            "Measurement instrument",
            "Instrument calibration/check data",
            "Tare method and tare mass",
            "Measurement operators",
            "Measurement dates",
            "Measurement reviewer and review reference",
            "Actual measured masses",
        ):
            self.assertIn(expected, descriptions)

    def test_committed_planning_packet_matches_deterministic_generator_and_schema(self) -> None:
        result = prepare_planning_packet(
            [
                "--manifest",
                str(MANIFEST_PATH),
                "--reviewed-mapping",
                str(MAPPING_PATH),
                "--output",
                str(PACKET_PATH),
            ]
        )
        self.assertEqual(result, 0)
        manifest_schema = json.loads(
            (ROOT / "schemas" / "portion-study-manifest-0.3.0.json").read_text(encoding="utf-8")
        )
        packet_schema = json.loads(
            (ROOT / "schemas" / "portion-planning-packet-0.1.0.json").read_text(encoding="utf-8")
        )
        packet = json.loads(PACKET_PATH.read_text(encoding="utf-8"))
        self.assertEqual(
            manifest_schema["properties"]["schema_version"]["const"], MANIFEST_SCHEMA_VERSION
        )
        self.assertFalse(manifest_schema["additionalProperties"])
        self.assertEqual(
            packet_schema["properties"]["schema_version"]["const"],
            PLANNING_PACKET_SCHEMA_VERSION,
        )
        self.assertFalse(packet_schema["additionalProperties"])
        _assert_json_schema_conforms(packet, packet_schema)
        generated, errors = build_portion_planning_packet(
            MANIFEST_PATH.read_bytes(), MAPPING_PATH.read_bytes()
        )
        self.assertEqual(errors, ())
        expected_bytes = canonical_json_bytes(generated)
        self.assertEqual(PACKET_PATH.read_bytes(), expected_bytes)
        self.assertEqual(
            packet["input_artifacts"]["reviewed_mapping"]["sha256"], REVIEWED_MAPPING_SHA256
        )
        self.assertEqual(
            hashlib.sha256(PACKET_PATH.read_bytes()).hexdigest(),
            hashlib.sha256(expected_bytes).hexdigest(),
        )

    def test_packet_schema_rejects_mapping_identity_backend_id_and_blocker_mutations(self) -> None:
        schema = json.loads(
            (ROOT / "schemas" / "portion-planning-packet-0.1.0.json").read_text(
                encoding="utf-8"
            )
        )
        packet = json.loads(PACKET_PATH.read_text(encoding="utf-8"))
        mutations = (
            (
                "wrong reviewed mapping hash",
                lambda value: value["input_artifacts"]["reviewed_mapping"].__setitem__(
                    "sha256", "0" * 64
                ),
            ),
            (
                "wrong reviewed evidence identity hash",
                lambda value: value["target"]["reviewed_evidence_identity"].__setitem__(
                    "sha256", "0" * 64
                ),
            ),
            (
                "non-null backend food ID",
                lambda value: value["target"].__setitem__("backend_food_id", "2708408"),
            ),
            (
                "wrong source identity",
                lambda value: value["target"]["reviewed_evidence_identity"]["source"].__setitem__(
                    "fdc_id", 2708409
                ),
            ),
            (
                "substituted blocker",
                lambda value: value["unresolved_human_inputs"][0].__setitem__(
                    "key", "unreviewed_extra_input"
                ),
            ),
            (
                "duplicate blocker replacing another",
                lambda value: value["unresolved_human_inputs"].__setitem__(
                    0, copy.deepcopy(value["unresolved_human_inputs"][1])
                ),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(label=label):
                changed = copy.deepcopy(packet)
                mutate(changed)
                with self.assertRaises(AssertionError):
                    _assert_json_schema_conforms(changed, schema)


_SCHEMA_KEYWORDS = {
    "$id",
    "$schema",
    "additionalProperties",
    "const",
    "items",
    "maxItems",
    "minItems",
    "oneOf",
    "pattern",
    "properties",
    "required",
    "type",
    "uniqueItems",
}


def _assert_json_schema_conforms(instance: Any, schema: dict[str, Any], path: str = "$") -> None:
    unsupported = set(schema) - _SCHEMA_KEYWORDS
    if unsupported:
        raise AssertionError(f"{path}: unsupported schema keywords {sorted(unsupported)}")

    expected_type = schema.get("type")
    if expected_type is not None and not _matches_schema_type(instance, expected_type):
        raise AssertionError(f"{path}: type mismatch; expected {expected_type}")
    if "const" in schema and not _typed_json_equal(instance, schema["const"]):
        raise AssertionError(f"{path}: const mismatch")
    if "pattern" in schema and (
        not isinstance(instance, str) or re.fullmatch(schema["pattern"], instance) is None
    ):
        raise AssertionError(f"{path}: pattern mismatch")

    if isinstance(instance, dict):
        required = schema.get("required", [])
        if not all(key in instance for key in required):
            raise AssertionError(f"{path}: required property is missing")
        properties = schema.get("properties", {})
        for key, value in instance.items():
            if key in properties:
                _assert_json_schema_conforms(value, properties[key], f"{path}.{key}")
            elif schema.get("additionalProperties") is False:
                raise AssertionError(f"{path}.{key}: additional property is forbidden")

    if isinstance(instance, list):
        if len(instance) < schema.get("minItems", 0):
            raise AssertionError(f"{path}: too few items")
        if len(instance) > schema.get("maxItems", len(instance)):
            raise AssertionError(f"{path}: too many items")
        if schema.get("uniqueItems") and len(
            {json.dumps(item, ensure_ascii=False, sort_keys=True) for item in instance}
        ) != len(instance):
            raise AssertionError(f"{path}: items are not unique")
        if "items" in schema:
            for index, item in enumerate(instance):
                _assert_json_schema_conforms(item, schema["items"], f"{path}[{index}]")

    if "oneOf" in schema:
        matches = 0
        for candidate in schema["oneOf"]:
            try:
                _assert_json_schema_conforms(instance, candidate, path)
            except AssertionError:
                continue
            matches += 1
        if matches != 1:
            raise AssertionError(f"{path}: expected exactly one matching schema")


def _matches_schema_type(instance: Any, expected: str) -> bool:
    if expected == "object":
        return isinstance(instance, dict)
    if expected == "array":
        return isinstance(instance, list)
    if expected == "string":
        return isinstance(instance, str)
    if expected == "integer":
        return type(instance) is int
    if expected == "null":
        return instance is None
    raise AssertionError(f"unsupported schema type {expected}")


def _typed_json_equal(actual: Any, expected: Any) -> bool:
    if type(actual) is not type(expected):
        return False
    if isinstance(expected, dict):
        return actual.keys() == expected.keys() and all(
            _typed_json_equal(actual[key], expected[key]) for key in expected
        )
    if isinstance(expected, list):
        return len(actual) == len(expected) and all(
            _typed_json_equal(left, right) for left, right in zip(actual, expected, strict=True)
        )
    return actual == expected
