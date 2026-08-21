from __future__ import annotations

from pathlib import Path

from .adapters.synthetic import SyntheticAdapter
from .artifacts import ArtifactStore
from .config import FactoryConfig
from .curation.queue import build_candidate_queue
from .normalization.synthetic import normalize_records
from .release.compiler import compile_candidate_package
from .source_registry import SourceRegistry
from .validation.rules import validate_records


def run_synthetic_pipeline(config_path: Path, fixture_path: Path, output_dir: Path) -> Path:
    config = FactoryConfig.load(config_path)
    registry = SourceRegistry.load(config.source_registry_path)
    metadata = registry.get("synthetic_fixture")
    registry.assert_test_use_allowed(metadata)
    payload = fixture_path.resolve().read_bytes()
    artifact = ArtifactStore(config.artifact_root).put_bytes(
        payload,
        content_type="application/json",
    )
    source_records = SyntheticAdapter().parse(payload, metadata)
    normalized_records = normalize_records(source_records)
    curation_queue = build_candidate_queue(normalized_records)
    validation = validate_records(normalized_records, metadata, artifact)
    return compile_candidate_package(
        output_dir=output_dir,
        metadata=metadata,
        artifact=artifact,
        records=normalized_records,
        curation_queue=curation_queue,
        validation=validation,
    )
