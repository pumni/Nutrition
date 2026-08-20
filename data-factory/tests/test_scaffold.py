from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.artifacts import (  # noqa: E402
    AcquisitionMetadata,
    ArtifactConflictError,
    ArtifactIntegrityError,
    ArtifactStore,
)
from nutrition_data_factory.models import SourceRecord, ValidationReport  # noqa: E402
from nutrition_data_factory.pipeline import run_synthetic_pipeline  # noqa: E402
from nutrition_data_factory.release.compiler import (  # noqa: E402
    ReleaseCompilationError,
    compile_candidate_package,
)
from nutrition_data_factory.source_registry import SourceRegistry, SourceRegistryError  # noqa: E402
from nutrition_data_factory.provenance import (  # noqa: E402
    json_schema_fingerprint,
    source_record_hash,
)
from nutrition_data_factory.validation.rules import validate_records  # noqa: E402


class ScaffoldTests(unittest.TestCase):
    def test_artifact_store_is_content_addressed_and_immutable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            payload = b"synthetic artifact"
            store = ArtifactStore(root / "artifacts")
            reference = store.put_bytes(payload, content_type="text/plain")
            self.assertEqual(reference.sha256, hashlib.sha256(payload).hexdigest())
            self.assertEqual(store.read(reference), payload)
            target = store.root / reference.relative_path
            target.write_bytes(b"tampered")
            with self.assertRaises(ArtifactConflictError):
                store.put_bytes(payload, content_type="text/plain")

    def test_artifact_integrity_metadata_is_verified_before_storage(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            payload = b"provenance fixture"
            digest = hashlib.sha256(payload).hexdigest()
            metadata = AcquisitionMetadata(
                source_code="synthetic_fixture",
                publisher="project-test-fixture",
                release="0.1.0",
                retrieved_at="2026-08-20T00:00:00Z",
                filename="synthetic-source.json",
                size=len(payload),
                content_type="application/json",
                sha256=digest,
                rights_state="test_only",
                acquisition_tool_version="test-tool-0.1.0",
            )
            reference = ArtifactStore(Path(temporary) / "artifacts").put_bytes(
                payload,
                content_type="application/json",
                expected_sha256=digest,
                expected_size=len(payload),
                acquisition=metadata,
            )
            self.assertEqual(reference.acquisition, metadata)
            with self.assertRaises(ArtifactIntegrityError):
                ArtifactStore(Path(temporary) / "other").put_bytes(
                    payload,
                    expected_sha256="f" * 64,
                )
            with self.assertRaises(ArtifactIntegrityError):
                ArtifactStore(Path(temporary) / "another").put_bytes(
                    payload,
                    expected_size=len(payload) + 1,
                )

    def test_provenance_hashes_are_deterministic_and_schema_fingerprint_is_value_free(self) -> None:
        first = SourceRecord(
            source_code="synthetic_fixture",
            release="0.1.0",
            source_id="synthetic-001",
            label="Fixture",
            attributes={"b": 2, "a": 1},
        )
        second = SourceRecord(
            source_code="synthetic_fixture",
            release="0.1.0",
            source_id="synthetic-001",
            label="Fixture",
            attributes={"a": 1, "b": 2},
        )
        self.assertEqual(source_record_hash(first), source_record_hash(second))
        shape_one = json_schema_fingerprint({"record": {"id": 1, "label": "alpha"}})
        shape_two = json_schema_fingerprint({"record": {"id": 9, "label": "beta"}})
        shape_three = json_schema_fingerprint({"record": {"id": 9, "label": "beta", "extra": True}})
        self.assertEqual(shape_one, shape_two)
        self.assertNotEqual(shape_one, shape_three)

    def test_synthetic_pipeline_ingest_normalize_validate_package(self) -> None:
        fixture = ROOT / "tests" / "fixtures" / "synthetic-source.json"
        with tempfile.TemporaryDirectory() as temporary:
            temporary_root = Path(temporary)
            config = temporary_root / "config.json"
            config.write_text(
                json.dumps(
                    {
                        "artifact_store": {"root": "artifacts"},
                        "source_metadata": {
                            "registry_path": str(ROOT / "config" / "source_registry.json")
                        },
                    }
                ),
                encoding="utf-8",
            )
            first = run_synthetic_pipeline(config, fixture, temporary_root / "release-a")
            second = run_synthetic_pipeline(config, fixture, temporary_root / "release-b")
            self.assertEqual(_package_bytes(first), _package_bytes(second))
            manifest = json.loads((first / "manifest.json").read_text(encoding="utf-8"))
            self.assertFalse(manifest["production_eligible"])
            self.assertEqual(manifest["record_counts"]["normalized_records"], 2)
            normalized = (first / "normalized-records.jsonl").read_text(encoding="utf-8")
            self.assertIn('"normalized_label":"fixture alpha"', normalized)
            self.assertNotRegex(normalized.lower(), r"calorie|protein|carbohydrate|nutrient")
            for line in (first / "checksums.sha256").read_text(encoding="utf-8").splitlines():
                digest, filename = line.split("  ", maxsplit=1)
                self.assertEqual(
                    digest,
                    hashlib.sha256((first / filename).read_bytes()).hexdigest(),
                )

    def test_handoff_seed_preserves_blocked_rights_states(self) -> None:
        registry = SourceRegistry.load(ROOT / "config" / "source_registry.json")
        self.assertEqual(registry.get("usda_fdc_foundation").rights_state, "approved")
        self.assertEqual(registry.get("vn_fct_2017").rights_state, "prohibited")
        self.assertEqual(registry.get("smiling_vietnam_2013").rights_state, "reference_only")
        self.assertEqual(registry.get("project_recipe").rights_state, "pending_unknown")
        registry.assert_can_contribute_to_production(registry.get("usda_fdc_foundation"))
        for code in (
            "usda_fndds",
            "fao_infoods_anfood_2",
            "smiling_vietnam_2013",
            "vn_fct_2017",
            "project_recipe",
        ):
            with self.assertRaises(SourceRegistryError):
                registry.assert_can_contribute_to_production(registry.get(code))

    def test_free_access_does_not_bypass_reference_only_rights(self) -> None:
        registry = SourceRegistry.load(ROOT / "config" / "source_registry.json")
        metadata = registry.get("fao_infoods_anfood_2")
        self.assertEqual(metadata.access_status, "free_access_reference_only")
        with self.assertRaises(SourceRegistryError):
            registry.assert_can_contribute_to_production(metadata)
        report = validate_records(
            [],
            metadata,
            _synthetic_artifact_reference(),
            production_candidate=True,
        )
        self.assertFalse(report.passed)
        self.assertTrue(any("reference_only" in error for error in report.errors))

    def test_compiler_rejects_non_approved_production_contributor(self) -> None:
        registry = SourceRegistry.load(ROOT / "config" / "source_registry.json")
        metadata = registry.get("smiling_vietnam_2013")
        validation = ValidationReport(
            rule_version="test",
            passed=True,
            errors=(),
            warnings=(),
            statistics={},
        )
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "release"
            with self.assertRaises(ReleaseCompilationError):
                compile_candidate_package(
                    output_dir=output,
                    metadata=metadata,
                    artifact=_synthetic_artifact_reference(),
                    records=[],
                    curation_queue=[],
                    validation=validation,
                    production_candidate=True,
                )
            self.assertFalse(output.exists())


def _package_bytes(path: Path) -> dict[str, bytes]:
    return {item.name: item.read_bytes() for item in sorted(path.iterdir()) if item.is_file()}


def _synthetic_artifact_reference():
    from nutrition_data_factory.artifacts import ArtifactRef

    return ArtifactRef(
        sha256="0" * 64,
        size=0,
        content_type="application/octet-stream",
        relative_path="00/" + "0" * 62,
    )


if __name__ == "__main__":
    unittest.main()
