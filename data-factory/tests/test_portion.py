from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.portion import (  # noqa: E402
    BOUND_POLICY_VERSION,
    ESTIMATOR_VERSION,
    MANIFEST_SCHEMA_VERSION,
    MEASUREMENTS_SCHEMA_VERSION,
    PORTION_POLICY_VERSION,
    PORTION_PROTOCOL_VERSION,
    build_portion_review_packet,
    compile_portion_study,
    validate_manifest,
)
sys.path.insert(0, str(ROOT / "scripts"))
from prepare_portion_review_packet import main as prepare_review_packet  # noqa: E402


# All measurements in this module are synthetic test fixtures, never candidate evidence.
MANIFEST = {
    "schema_version": MANIFEST_SCHEMA_VERSION,
    "study_id": "study-fixture-001",
    "target": {
        "identity": {
            "status": "reviewed",
            "food_concept_id": "fixture-only-food-id",
            "reviewer": "reviewer-1",
            "review_ref": "test://identity-review",
        },
        "preparation": {
            "status": "reviewed",
            "definition": "fixture-only preparation",
            "reviewer": "reviewer-1",
            "review_ref": "test://preparation-review",
        },
        "measure": {
            "canonical_measure": "fixture-vessel",
            "original_unit_phrase": "fixture cup",
            "represented_quantity": 1,
            "context_status": "reviewed",
            "physical_context": "fixture vessel with fixed capacity and fill protocol",
            "context_reviewer": "reviewer-1",
            "context_review_ref": "test://context-review",
        },
    },
    "study_plan": {
        "minimum_independent_samples": 2,
        "minimum_batches": 2,
        "reviewer": "reviewer-1",
        "review_ref": "test://study-plan-review",
    },
    "protocol_version": PORTION_PROTOCOL_VERSION,
    "policy_version": PORTION_POLICY_VERSION,
    "estimator_version": ESTIMATOR_VERSION,
    "bound_policy_version": BOUND_POLICY_VERSION,
    "instrument": {
        "instrument_id": "scale-fixture-1",
        "resolution_g": 1,
        "calibration": {
            "reference": "calibration-fixture-1",
            "checked_at": "2026-01-01T09:00:00+07:00",
            "standard_mass_g": 100,
            "observed_mass_g": 100,
            "tolerance_g": 0,
            "operator_id": "operator-1",
        },
    },
    "tare": {"method": "fixture tare procedure", "mass_g": 5},
    "operator_ids": ["operator-1"],
}


def observations() -> list[dict[str, object]]:
    target = {
        "food_concept_id": "fixture-only-food-id",
        "preparation": "fixture-only preparation",
        "canonical_measure": "fixture-vessel",
        "represented_quantity": 1,
        "physical_context": "fixture vessel with fixed capacity and fill protocol",
    }
    common = {
        "batch_id": "batch-1",
        "target": target,
        "instrument_id": "scale-fixture-1",
        "tare_applied": True,
        "tare_mass_g": 5,
        "calibration_reference": "calibration-fixture-1",
        "operator_id": "operator-1",
        "measured_at": "2026-01-02T09:00:00+07:00",
    }
    return [
        {
            **common,
            "observation_id": "w1",
            "sample_id": "sample-1",
            "sample_kind": "independent_sample",
            "repeat_of_observation_id": None,
            "mass_g": 100,
        },
        {
            **common,
            "observation_id": "w2",
            "sample_id": "sample-1",
            "sample_kind": "repeat_weighing",
            "repeat_of_observation_id": "w1",
            "mass_g": 102,
        },
        {
            **common,
            "batch_id": "batch-2",
            "observation_id": "w3",
            "sample_id": "sample-2",
            "sample_kind": "independent_sample",
            "repeat_of_observation_id": None,
            "mass_g": 110,
        },
        {
            **common,
            "batch_id": "batch-2",
            "observation_id": "w4",
            "sample_id": "sample-2",
            "sample_kind": "repeat_weighing",
            "repeat_of_observation_id": "w3",
            "mass_g": 111,
        },
    ]


def measurement_document(
    rows: list[dict[str, object]] | None = None,
    accounting: dict[str, object] | None = None,
) -> dict[str, object]:
    rows = observations() if rows is None else rows
    if accounting is None:
        accounting = {
            "all_samples_accounted_for": True,
            "collected_sample_ids": list(
                dict.fromkeys(
                    row["sample_id"] for row in rows if row["sample_kind"] == "independent_sample"
                )
            ),
            "deviations_or_exclusions": [],
        }
    return {
        "schema_version": MEASUREMENTS_SCHEMA_VERSION,
        "observations": rows,
        "measurement_accounting": accounting,
    }


def errors_with_reason(errors: tuple[dict[str, object], ...], reason: str) -> bool:
    return any(error.get("reason_code") == reason for error in errors)


class PortionTests(unittest.TestCase):
    def test_manifest_requires_study_plan_instrument_tare_and_reviewed_target_metadata(self) -> None:
        manifest_schema = json.loads(
            (ROOT / "schemas" / "portion-study-manifest-0.2.0.json").read_text(encoding="utf-8")
        )
        observations_schema = json.loads(
            (ROOT / "schemas" / "portion-measurements-0.2.0.json").read_text(encoding="utf-8")
        )
        self.assertEqual(
            manifest_schema["properties"]["schema_version"]["const"],
            MANIFEST_SCHEMA_VERSION,
        )
        self.assertEqual(
            observations_schema["properties"]["schema_version"]["const"],
            MEASUREMENTS_SCHEMA_VERSION,
        )
        self.assertFalse(manifest_schema["additionalProperties"])
        self.assertFalse(observations_schema["additionalProperties"])
        self.assertIn("measurement_accounting", observations_schema["required"])

        manifest, errors = validate_manifest(MANIFEST)
        self.assertIsNotNone(manifest)
        self.assertEqual(errors, ())

        missing_tare = copy.deepcopy(MANIFEST)
        del missing_tare["tare"]["mass_g"]
        _, errors = validate_manifest(missing_tare)
        self.assertTrue(errors_with_reason(errors, "missing_tare_field"))

        missing_resolution = copy.deepcopy(MANIFEST)
        del missing_resolution["instrument"]["resolution_g"]
        _, errors = validate_manifest(missing_resolution)
        self.assertTrue(errors_with_reason(errors, "missing_instrument_field"))

        missing_context_review = copy.deepcopy(MANIFEST)
        del missing_context_review["target"]["measure"]["context_review_ref"]
        _, errors = validate_manifest(missing_context_review)
        self.assertTrue(errors_with_reason(errors, "missing_measure_field"))

        unresolved_with_identity = copy.deepcopy(MANIFEST)
        unresolved_with_identity["target"]["identity"]["status"] = "unresolved"
        _, errors = validate_manifest(unresolved_with_identity)
        self.assertTrue(errors_with_reason(errors, "unresolved_decision_has_value"))

    def test_repeats_are_linked_and_do_not_inflate_independent_sample_count(self) -> None:
        self.assertEqual(MANIFEST["schema_version"], "portion-study-manifest-0.2.0")
        raw_observations = observations()
        before = copy.deepcopy(raw_observations)
        result = compile_portion_study(MANIFEST, measurement_document(raw_observations))
        replay = compile_portion_study(MANIFEST, measurement_document(copy.deepcopy(raw_observations)))

        self.assertTrue(result.passed)
        self.assertEqual(result.sample_count, 2)
        self.assertEqual(result.batch_count, 2)
        self.assertEqual(result.repeat_weighing_count, 2)
        self.assertEqual(result.central_mass_g, 105.75)
        self.assertEqual(result.lower_mass_g, 101)
        self.assertEqual(result.upper_mass_g, 110.5)
        self.assertLessEqual(result.lower_mass_g, result.central_mass_g)
        self.assertLessEqual(result.central_mass_g, result.upper_mass_g)
        self.assertEqual(result.observations_hash, replay.observations_hash)
        self.assertEqual(result.to_dict(), replay.to_dict())
        self.assertFalse(result.review_ready)
        self.assertFalse(result.publishable)
        self.assertIn("measurement_review_missing", result.publication_blockers)
        self.assertIn("publication_disabled_tooling_only", result.publication_blockers)
        self.assertEqual(raw_observations, before)

    def test_empty_deviation_list_explicitly_accounts_for_all_collected_samples(self) -> None:
        document = measurement_document()
        result = compile_portion_study(
            MANIFEST,
            document,
            reviewer_approval_ref="test://measurement-review",
            reviewer="reviewer-fixture",
        )
        self.assertTrue(result.passed)
        self.assertTrue(result.review_ready)
        self.assertEqual(result.measurement_accounting["deviations_or_exclusions"], [])
        packet = build_portion_review_packet(result, measurements_sha256="d" * 64)
        self.assertEqual(packet["measurement_accounting"], document["measurement_accounting"])
        self.assertEqual(packet["input_artifacts"]["measurements"]["sha256"], "d" * 64)
        self.assertFalse(packet["publication"]["publishable"])

    def test_explained_deviation_and_exclusion_are_accounted_without_counting_excluded_sample(self) -> None:
        accounting = {
            "all_samples_accounted_for": True,
            "collected_sample_ids": ["sample-1", "sample-2", "sample-3"],
            "deviations_or_exclusions": [
                {
                    "kind": "deviation",
                    "sample_id": "sample-1",
                    "reason": "fixture handling note",
                    "reference": "test://deviation/sample-1",
                },
                {
                    "kind": "exclusion",
                    "sample_id": "sample-3",
                    "reason": "fixture sample was damaged before weighing",
                    "reference": "test://exclusion/sample-3",
                },
            ],
        }
        result = compile_portion_study(
            MANIFEST,
            measurement_document(accounting=accounting),
            reviewer_approval_ref="test://measurement-review",
            reviewer="reviewer-fixture",
        )
        self.assertTrue(result.passed)
        self.assertTrue(result.review_ready)
        self.assertEqual(result.sample_count, 2)
        self.assertEqual(result.measurement_accounting, accounting)

    def test_incomplete_or_unexplained_sample_accounting_fails_closed(self) -> None:
        unaccounted = compile_portion_study(
            MANIFEST,
            observations(),
            reviewer_approval_ref="test://measurement-review",
            reviewer="reviewer-fixture",
        )
        self.assertTrue(errors_with_reason(unaccounted.errors, "measurement_accounting_required"))
        self.assertIsNone(unaccounted.central_mass_g)
        self.assertFalse(unaccounted.review_ready)

        incomplete_accounting = {
            "all_samples_accounted_for": False,
            "collected_sample_ids": ["sample-1", "sample-2", "sample-3"],
            "deviations_or_exclusions": [],
        }
        incomplete = compile_portion_study(
            MANIFEST,
            measurement_document(accounting=incomplete_accounting),
            reviewer_approval_ref="test://measurement-review",
            reviewer="reviewer-fixture",
        )
        self.assertTrue(errors_with_reason(incomplete.errors, "sample_accounting_incomplete"))
        self.assertTrue(
            errors_with_reason(incomplete.errors, "collected_sample_missing_observation_or_exclusion")
        )
        self.assertIsNone(incomplete.central_mass_g)
        self.assertFalse(incomplete.review_ready)

        unexplained_accounting = {
            "all_samples_accounted_for": True,
            "collected_sample_ids": ["sample-1", "sample-2", "sample-3"],
            "deviations_or_exclusions": [
                {"kind": "exclusion", "sample_id": "sample-3", "reason": "", "reference": ""}
            ],
        }
        unexplained = compile_portion_study(
            MANIFEST,
            measurement_document(accounting=unexplained_accounting),
            reviewer_approval_ref="test://measurement-review",
            reviewer="reviewer-fixture",
        )
        self.assertTrue(errors_with_reason(unexplained.errors, "accounting_reason_required"))
        self.assertTrue(errors_with_reason(unexplained.errors, "accounting_reference_required"))
        self.assertIsNone(unexplained.central_mass_g)
        self.assertFalse(unexplained.review_ready)

    def test_malformed_negative_and_zero_masses_fail_closed(self) -> None:
        for invalid_mass in ("100", -1, 0, float("inf"), float("nan"), True):
            with self.subTest(invalid_mass=invalid_mass):
                invalid = observations()
                invalid[0]["mass_g"] = invalid_mass
                result = compile_portion_study(MANIFEST, measurement_document(invalid))
                self.assertFalse(result.errors == ())
                self.assertIsNone(result.central_mass_g)
                self.assertTrue(errors_with_reason(result.errors, "mass_g_must_be_positive_finite"))

    def test_repeats_must_reference_their_own_independent_sample(self) -> None:
        missing_parent = observations()
        missing_parent[1]["repeat_of_observation_id"] = "not-a-primary-observation"
        result = compile_portion_study(MANIFEST, measurement_document(missing_parent))
        self.assertTrue(errors_with_reason(result.errors, "repeat_parent_not_independent_sample"))
        self.assertIsNone(result.central_mass_g)

        wrong_sample = observations()
        wrong_sample[1]["sample_id"] = "sample-2"
        result = compile_portion_study(MANIFEST, measurement_document(wrong_sample))
        self.assertTrue(errors_with_reason(result.errors, "repeat_sample_id_mismatch"))

        wrong_batch = observations()
        wrong_batch[1]["batch_id"] = "batch-2"
        result = compile_portion_study(MANIFEST, measurement_document(wrong_batch))
        self.assertTrue(errors_with_reason(result.errors, "repeat_batch_id_mismatch"))
        self.assertIsNone(result.central_mass_g)

    def test_insufficient_samples_and_batches_do_not_meet_the_approved_plan(self) -> None:
        rows = observations()[:2]
        rows[1]["batch_id"] = "repeat-only-batch"
        result = compile_portion_study(MANIFEST, measurement_document(rows))
        self.assertTrue(errors_with_reason(result.errors, "insufficient_independent_samples"))
        self.assertTrue(errors_with_reason(result.errors, "insufficient_independent_batches"))
        self.assertIsNone(result.central_mass_g)

    def test_missing_measurements_produce_a_blocked_packet_without_estimates(self) -> None:
        result = compile_portion_study(MANIFEST, [])
        self.assertTrue(errors_with_reason(result.errors, "observations_required"))
        self.assertIn("measurement_data_missing", result.publication_blockers)
        self.assertIsNone(result.central_mass_g)
        self.assertIsNone(result.lower_mass_g)
        self.assertIsNone(result.upper_mass_g)
        self.assertFalse(result.publishable)

    def test_instrument_tare_calibration_and_context_must_match_the_manifest(self) -> None:
        invalid = observations()
        invalid[0]["instrument_id"] = "different-scale"
        invalid[1]["tare_applied"] = False
        invalid[2]["tare_mass_g"] = 6
        invalid[3]["target"] = {**invalid[3]["target"], "physical_context": "different context"}
        result = compile_portion_study(MANIFEST, measurement_document(invalid))
        self.assertTrue(errors_with_reason(result.errors, "instrument_mismatch"))
        self.assertTrue(errors_with_reason(result.errors, "tare_not_confirmed"))
        self.assertTrue(errors_with_reason(result.errors, "tare_mass_mismatch"))
        self.assertTrue(errors_with_reason(result.errors, "target_context_mismatch"))
        self.assertEqual(
            [(error.get("observation_index"), error["reason_code"]) for error in result.errors],
            [
                (0, "instrument_mismatch"),
                (1, "tare_not_confirmed"),
                (2, "tare_mass_mismatch"),
                (3, "target_context_mismatch"),
            ],
        )
        self.assertIsNone(result.central_mass_g)

        missing_metadata = observations()
        del missing_metadata[0]["tare_mass_g"]
        del missing_metadata[1]["instrument_id"]
        result = compile_portion_study(MANIFEST, measurement_document(missing_metadata))
        self.assertTrue(errors_with_reason(result.errors, "missing_observation_field"))
        self.assertIsNone(result.central_mass_g)

        invalid_manifest = copy.deepcopy(MANIFEST)
        invalid_manifest["instrument"]["calibration"]["observed_mass_g"] = 102
        _, errors = validate_manifest(invalid_manifest)
        self.assertTrue(errors_with_reason(errors, "calibration_check_out_of_tolerance"))

    def test_unresolved_identity_or_context_never_compiles_a_mass_estimate(self) -> None:
        unresolved = copy.deepcopy(MANIFEST)
        unresolved["target"]["identity"] = {
            "status": "unresolved",
            "food_concept_id": None,
            "reviewer": None,
            "review_ref": None,
        }
        unresolved["target"]["preparation"] = {
            "status": "unresolved",
            "definition": None,
            "reviewer": None,
            "review_ref": None,
        }
        unresolved["target"]["measure"].update(
            {
                "context_status": "unresolved",
                "physical_context": None,
                "context_reviewer": None,
                "context_review_ref": None,
            }
        )
        rows = observations()
        unresolved_target = {
            "food_concept_id": None,
            "preparation": None,
            "canonical_measure": "fixture-vessel",
            "represented_quantity": 1,
            "physical_context": None,
        }
        for row in rows:
            row["target"] = unresolved_target
        result = compile_portion_study(unresolved, measurement_document(rows))
        self.assertTrue(result.passed)
        self.assertIsNone(result.central_mass_g)
        self.assertIn("food_identity_unresolved", result.publication_blockers)
        self.assertIn("preparation_state_unresolved", result.publication_blockers)
        self.assertIn("physical_measure_context_unresolved", result.publication_blockers)
        self.assertFalse(result.publishable)

    def test_unrecognized_model_gram_field_is_rejected(self) -> None:
        invalid = observations()
        invalid[0]["gram_weight"] = 1000
        result = compile_portion_study(MANIFEST, measurement_document(invalid))
        self.assertTrue(errors_with_reason(result.errors, "unknown_field"))
        self.assertIsNone(result.central_mass_g)
        self.assertFalse(result.publishable)

    def test_human_measurement_review_is_recorded_but_publication_stays_blocked(self) -> None:
        result = compile_portion_study(
            MANIFEST,
            measurement_document(),
            reviewer_approval_ref="review://portion/fixture-1",
            reviewer="reviewer-1",
        )
        self.assertTrue(result.review_ready)
        self.assertFalse(result.publishable)
        packet = build_portion_review_packet(
            result,
            manifest_sha256="a" * 64,
            measurements_sha256="b" * 64,
            compiler_sha256="c" * 64,
        )
        replay = build_portion_review_packet(
            result,
            manifest_sha256="a" * 64,
            measurements_sha256="b" * 64,
            compiler_sha256="c" * 64,
        )
        self.assertEqual(packet, replay)
        self.assertEqual(packet["status"], "review_ready_publication_blocked")
        self.assertEqual(packet["measurement_review"]["review_ref"], "review://portion/fixture-1")
        self.assertEqual(packet["publication"]["status"], "blocked")
        self.assertFalse(packet["publication"]["publishable"])
        self.assertFalse(packet["publication"]["release_created"])
        self.assertFalse(packet["publication"]["activation_attempted"])

    def test_machine_reviewer_and_wrong_measurement_schema_are_rejected(self) -> None:
        result = compile_portion_study(
            MANIFEST,
            measurement_document(),
            reviewer_approval_ref="review://portion/fixture-1",
            reviewer="codex-agent",
        )
        self.assertTrue(errors_with_reason(result.errors, "machine_reviewer_forbidden"))

        wrong_schema = {
            "schema_version": "portion-measurements-9.9.9",
            "observations": observations(),
            "measurement_accounting": measurement_document()["measurement_accounting"],
        }
        result = compile_portion_study(MANIFEST, wrong_schema)
        self.assertTrue(errors_with_reason(result.errors, "unsupported_measurements_schema"))
        self.assertIsNone(result.central_mass_g)

    def test_packet_cli_retains_hash_pinned_inputs_and_never_overwrites_a_packet(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            manifest_path = root / "manifest.json"
            measurements_path = root / "measurements.json"
            output_path = root / "packet.json"
            artifact_store = root / "artifacts"
            manifest_bytes = (json.dumps(MANIFEST, sort_keys=True, ensure_ascii=False) + "\n").encode("utf-8")
            measurements_payload = {
                "schema_version": MEASUREMENTS_SCHEMA_VERSION,
                "observations": observations(),
                "measurement_accounting": measurement_document()["measurement_accounting"],
            }
            measurements_bytes = (
                json.dumps(measurements_payload, sort_keys=True, ensure_ascii=False) + "\n"
            ).encode("utf-8")
            manifest_path.write_bytes(manifest_bytes)
            measurements_path.write_bytes(measurements_bytes)
            args = [
                "--manifest",
                str(manifest_path),
                "--measurements",
                str(measurements_path),
                "--artifact-store",
                str(artifact_store),
                "--output",
                str(output_path),
            ]
            with redirect_stdout(StringIO()):
                self.assertEqual(prepare_review_packet(args), 0)
            original_packet = output_path.read_bytes()
            packet = json.loads(original_packet)
            manifest_ref = packet["input_artifacts"]["manifest"]
            measurements_ref = packet["input_artifacts"]["measurements"]
            self.assertEqual((artifact_store / manifest_ref["relative_path"]).read_bytes(), manifest_bytes)
            self.assertEqual((artifact_store / measurements_ref["relative_path"]).read_bytes(), measurements_bytes)
            self.assertFalse(packet["publication"]["publishable"])
            self.assertEqual(packet["measurement_accounting"], measurements_payload["measurement_accounting"])

            with redirect_stdout(StringIO()):
                self.assertEqual(prepare_review_packet(args), 0)
            self.assertEqual(output_path.read_bytes(), original_packet)

            measurements_path.write_text(json.dumps(measurements_payload, indent=2), encoding="utf-8")
            with redirect_stdout(StringIO()):
                with self.assertRaisesRegex(RuntimeError, "refusing to overwrite"):
                    prepare_review_packet(args)


if __name__ == "__main__":
    unittest.main()
