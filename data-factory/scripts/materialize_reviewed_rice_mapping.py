from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.curation.reviewed_mapping import (  # noqa: E402
    materialize_reviewed_rice_mapping,
    sha256,
)


REPORT_PATH = ROOT / "docs" / "reviews" / "fndds-secondary-source-issue-44.json"
SOURCE_REGISTRY_PATH = ROOT / "config" / "source_registry.json"
DECISION_PATH = ROOT / "docs" / "reviews" / "vietnamese-basic-food-identity-issue-35-decisions-0.2.0.jsonl"
MAPPING_PATH = ROOT / "docs" / "reviews" / "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json"


def main() -> int:
    try:
        decision_bytes, mapping_bytes = materialize_reviewed_rice_mapping(
            REPORT_PATH.read_bytes(), SOURCE_REGISTRY_PATH.read_bytes()
        )
        _write_immutable(DECISION_PATH, decision_bytes)
        _write_immutable(MAPPING_PATH, mapping_bytes)
    except (OSError, ValueError) as error:
        print(f"reviewed rice mapping materialization failed: {error}", file=sys.stderr)
        return 2

    print(f"decision={DECISION_PATH.relative_to(ROOT)} sha256={sha256(decision_bytes)}")
    print(f"mapping={MAPPING_PATH.relative_to(ROOT)} sha256={sha256(mapping_bytes)}")
    return 0


def _write_immutable(path: Path, content: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        if path.read_bytes() != content:
            raise ValueError(f"refusing to overwrite a different immutable artifact: {path}")
        return
    path.write_bytes(content)


if __name__ == "__main__":
    raise SystemExit(main())
