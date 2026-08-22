from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.adapters.fdc_foundation import FdcSourceRecord  # noqa: E402
from nutrition_data_factory.artifacts import ArtifactRef  # noqa: E402
from nutrition_data_factory.compatibility import load_compatibility_manifest  # noqa: E402
from nutrition_data_factory.release.catalog_handoff_v1 import (  # noqa: E402
    TEST_FIXTURE_PROFILE,
    SUPPORTED_NUTRIENT_CODES,
    compile_catalog_handoff_v1,
)
from nutrition_data_factory.source_registry import SourceRegistry  # noqa: E402


SELECTION = load_compatibility_manifest(ROOT / "config" / "backend-fdc-selection.json")


def fixture_inputs() -> tuple[SourceMetadata, ArtifactRef, ArtifactRef, list[FdcSourceRecord], list[dict], list[dict], list[dict], list[dict]]:
    records = []
    compositions = []
    concepts = []
    names = []
    mappings = []
    for index, fdc_id in enumerate(SELECTION["fdc_ids"]):
        protein = float(index + 1)
        fat = float(index + 2)
        carbohydrate = float(index + 3)
        energy = float(40 + index)
        nutrients = (
            {"amount": protein, "nutrient": {"id": 1003, "name": "Protein", "unitName": "G"}},
            {"amount": fat, "nutrient": {"id": 1004, "name": "Total lipid (fat)", "unitName": "G"}},
            {"amount": carbohydrate, "nutrient": {"id": 1005, "name": "Carbohydrate, by difference", "unitName": "G"}},
            {"amount": energy, "nutrient": {"id": 2048, "name": "Energy (Atwater Specific)", "unitName": "KCAL"}},
        )
        payload = {
            "fdcId": fdc_id,
            "dataType": "Foundation",
            "description": f"Synthetic handoff fixture {fdc_id}",
            "foodNutrients": list(nutrients),
        }
        compositions.extend([
            {"source_id": str(fdc_id), "target_code": "protein_g", "source_nutrient_id": 1003, "source_label": "Protein", "source_unit": "G", "source_method": "declared_or_analytical", "value": protein, "value_status": "numeric"},
            {"source_id": str(fdc_id), "target_code": "fat_g", "source_nutrient_id": 1004, "source_label": "Total lipid (fat)", "source_unit": "G", "source_method": "declared_or_analytical", "value": fat, "value_status": "numeric"},
            {"source_id": str(fdc_id), "target_code": "carbohydrate_g", "source_nutrient_id": 1005, "source_label": "Carbohydrate, by difference", "source_unit": "G", "source_method": "declared_or_analytical", "value": carbohydrate, "value_status": "numeric"},
            {"source_id": str(fdc_id), "target_code": "energy_kcal", "source_nutrient_id": 2048, "source_label": "Energy (Atwater Specific)", "source_unit": "KCAL", "source_method": "atwater_specific", "value": energy, "value_status": "numeric"},
        ])
        payload_hash = hashlib.sha256(json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        record = FdcSourceRecord(fdc_id, payload["description"], "Foundation", [], nutrients, (), payload, payload_hash)
        records.append(record)
        concept_id = f"source-food:synthetic_fixture:{fdc_id}"
        concepts.append({"source_id": str(fdc_id), "concept_id": concept_id})
        names.append({"source_id": str(fdc_id), "name": payload["description"]})
        mappings.append({"source_id": str(fdc_id), "policy_version": "test-handoff-policy-0.1.0"})
    metadata = SourceRegistry.load(ROOT / "config" / "source_registry.json").get("synthetic_fixture")
    archive = ArtifactRef("a" * 64, 1, "application/zip", "archive.zip")
    extracted = ArtifactRef("b" * 64, 1, "application/json", "extracted.json")
    return metadata, archive, extracted, records, compositions, concepts, names, mappings


def compile_fixture(output: Path, **overrides: object) -> Path:
    metadata, archive, extracted, records, compositions, concepts, names, mappings = fixture_inputs()
    values = {"metadata": metadata, "archive_artifact": archive, "extracted_artifact": extracted, "source_records": records, "composition_values": compositions, "food_concepts": concepts, "food_names": names, "source_food_mappings": mappings, "backend_baseline": "test-baseline", "profile": TEST_FIXTURE_PROFILE, "source_schema_fingerprint": "synthetic-handoff-fixture-0.1.0"}
    values.update(overrides)
    return compile_catalog_handoff_v1(output, **values)  # type: ignore[arg-type]


class CatalogHandoffV1Tests(unittest.TestCase):
    def test_valid_package_has_exact_core_and_safe_flags(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            package = compile_fixture(Path(directory) / "package")
            self.assertEqual(sorted(path.name for path in package.iterdir()), ["checksums.sha256", "composition-values.jsonl", "dataset-release.json", "food-concepts.jsonl", "food-names.jsonl", "manifest.json", "raw-source-records.jsonl", "source-food-mappings.jsonl", "source-releases.json"])
            manifest = json.loads((package / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["contract_version"], "catalog-handoff-1.0.0")
            self.assertEqual(manifest["handoff_profile"], TEST_FIXTURE_PROFILE)
            self.assertEqual(manifest["selection"]["record_count"], 20)
            self.assertFalse(manifest["selection"]["auto_add_records"])
            self.assertFalse(manifest["production_eligible"])
            self.assertFalse(manifest["activation_authorized"])
            self.assertEqual(len((package / "raw-source-records.jsonl").read_text(encoding="utf-8").splitlines()), 20)
            values = [json.loads(line) for line in (package / "composition-values.jsonl").read_text(encoding="utf-8").splitlines()]
            self.assertEqual({value["target_code"] for value in values}, SUPPORTED_NUTRIENT_CODES)
            self.assertEqual(len(values), 80)

    def test_committed_golden_fixture_is_reproducible(self) -> None:
        fixture = ROOT.parent / "contracts" / "catalog-handoff" / "v1" / "fixtures" / "minimal-valid"
        with tempfile.TemporaryDirectory() as directory:
            generated = compile_fixture(Path(directory) / "generated")
            for path in fixture.iterdir():
                self.assertEqual(path.read_bytes(), (generated / path.name).read_bytes(), path.name)

    def test_compilation_is_byte_deterministic_and_input_order_independent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first = compile_fixture(Path(directory) / "first")
            metadata, archive, extracted, records, compositions, concepts, names, mappings = fixture_inputs()
            second = compile_catalog_handoff_v1(Path(directory) / "second", metadata=metadata, archive_artifact=archive, extracted_artifact=extracted, source_records=list(reversed(records)), composition_values=list(reversed(compositions)), food_concepts=list(reversed(concepts)), food_names=list(reversed(names)), source_food_mappings=list(reversed(mappings)), backend_baseline="test-baseline", profile=TEST_FIXTURE_PROFILE, source_schema_fingerprint="synthetic-handoff-fixture-0.1.0")
            for first_path in first.iterdir():
                self.assertEqual(first_path.read_bytes(), (second / first_path.name).read_bytes(), first_path.name)

    def test_unsupported_nutrients_are_not_exported(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            extra = {"source_id": "1750339", "target_code": "calcium_mg", "source_nutrient_id": 1087, "value": 1.0}
            package = compile_fixture(Path(directory) / "package", composition_values=fixture_inputs()[4] + [extra])
            values = [json.loads(line) for line in (package / "composition-values.jsonl").read_text(encoding="utf-8").splitlines()]
            self.assertNotIn("calcium_mg", {value["target_code"] for value in values})

    def test_duplicate_records_and_dangling_references_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            inputs = fixture_inputs()
            with self.assertRaises(ValueError):
                compile_fixture(Path(directory) / "duplicate", source_records=inputs[3] + [inputs[3][0]])
            with self.assertRaises(ValueError):
                compile_fixture(Path(directory) / "dangling", food_concepts=inputs[5][:-1])

    def test_real_registry_identity_and_source_grounding_are_required(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            inputs = fixture_inputs()
            with self.assertRaises(ValueError):
                compile_fixture(
                    Path(directory) / "legacy-source-code",
                    metadata=replace(inputs[0], code="usda_fdc"),
                )

            compositions = [dict(value) for value in inputs[4]]
            compositions[0]["value"] = 999.0
            with self.assertRaises(ValueError):
                compile_fixture(Path(directory) / "ungrounded-composition", composition_values=compositions)

            compositions = [dict(value) for value in inputs[4]]
            compositions[0]["source_method"] = "made_up_method"
            with self.assertRaises(ValueError):
                compile_fixture(Path(directory) / "ungrounded-method", composition_values=compositions)

            with self.assertRaises(ValueError):
                compile_fixture(Path(directory) / "incomplete-profile", composition_values=inputs[4][:-1])

    def test_composition_state_and_source_metadata_flags_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            inputs = fixture_inputs()
            compositions = [dict(value) for value in inputs[4]]
            compositions[0]["value_status"] = "zero"
            with self.assertRaises(ValueError):
                compile_fixture(Path(directory) / "wrong-zero-state", composition_values=compositions)

            with self.assertRaises(ValueError):
                compile_fixture(
                    Path(directory) / "production-source",
                    metadata=replace(inputs[0], production_eligible=True),
                )

            with self.assertRaises(ValueError):
                compile_fixture(
                    Path(directory) / "unapproved-rights",
                    metadata=replace(inputs[0], rights_state="prohibited"),
                )

            foundation = SourceRegistry.load(ROOT / "config" / "source_registry.json").get("usda_fdc_foundation")
            with self.assertRaises(ValueError):
                compile_catalog_handoff_v1(
                    Path(directory) / "fdc-unapproved-rights",
                    metadata=replace(foundation, rights_state="reference_only"),
                    archive_artifact=inputs[1],
                    extracted_artifact=inputs[2],
                    source_records=inputs[3],
                    composition_values=inputs[4],
                    food_concepts=inputs[5],
                    food_names=inputs[6],
                    source_food_mappings=inputs[7],
                    backend_baseline="test-baseline",
                )

    def test_unsafe_selection_flags_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            unsafe_selection = Path(directory) / "selection.json"
            selection = dict(SELECTION)
            selection["production_eligible"] = True
            unsafe_selection.write_text(json.dumps(selection), encoding="utf-8")
            with self.assertRaises(ValueError):
                compile_fixture(Path(directory) / "unsafe", selection_path=unsafe_selection)


if __name__ == "__main__":
    unittest.main()
