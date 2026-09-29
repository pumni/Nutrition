from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.curation.fndds_review import (  # noqa: E402
    load_fndds_review_policy,
)
from nutrition_data_factory.curation.reviewed_composition import (  # noqa: E402
    REVIEWED_COMPOSITION_REF,
    ReviewedCompositionError,
    materialize_reviewed_rice_composition,
    write_immutable_artifact,
)
from nutrition_data_factory.source_registry import SourceRegistry  # noqa: E402


POLICY_PATH = ROOT / "config" / "fndds-secondary-review-policy.json"
SOURCE_REGISTRY_PATH = ROOT / "config" / "source_registry.json"
REVIEWED_MAPPING_PATH = (
    ROOT / "docs" / "reviews" / "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json"
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Materialize evaluation-only rice composition from the pinned FNDDS archive"
    )
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)

    try:
        archive_bytes = args.archive.read_bytes()
        mapping_bytes = REVIEWED_MAPPING_PATH.read_bytes()
        loaded_policy = load_fndds_review_policy(POLICY_PATH)
        source_registry = SourceRegistry.load(SOURCE_REGISTRY_PATH)
        artifact_bytes = materialize_reviewed_rice_composition(
            archive_bytes,
            args.archive.name,
            mapping_bytes,
            loaded_policy,
            source_registry,
        )
        write_immutable_artifact(args.output, artifact_bytes)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"reviewed rice composition failed: {error}", file=sys.stderr)
        return 2

    print(
        f"artifact={REVIEWED_COMPOSITION_REF} output={args.output} "
        f"sha256={hashlib.sha256(artifact_bytes).hexdigest()}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
