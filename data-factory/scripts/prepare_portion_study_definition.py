from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.portion_study_definition import (  # noqa: E402
    STUDY_DEFINITION_REF,
    STRATEGY_DECISION_REF,
    build_study_definition,
    canonical_json_bytes,
    option_a_decision_bytes,
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Materialize the approved Option A decision and an unresolved study-definition shell"
    )
    parser.add_argument(
        "--mapping",
        type=Path,
        default=ROOT
        / "docs"
        / "reviews"
        / "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json",
    )
    parser.add_argument(
        "--previous-manifest",
        type=Path,
        default=ROOT / "config" / "portion-study-com-trang-bat-0.3.0.json",
    )
    parser.add_argument(
        "--previous-planning-packet",
        type=Path,
        default=ROOT
        / "docs"
        / "reviews"
        / "vietnamese-com-trang-bat-portion-planning-0.1.0.json",
    )
    parser.add_argument(
        "--decision-output",
        type=Path,
        default=ROOT
        / "docs"
        / "reviews"
        / "vietnamese-com-trang-bat-study-strategy-decision-0.1.0.json",
    )
    parser.add_argument(
        "--definition-output",
        type=Path,
        default=ROOT
        / "docs"
        / "reviews"
        / "vietnamese-com-trang-bat-study-definition-0.1.0.json",
    )
    args = parser.parse_args(argv)

    decision_bytes = option_a_decision_bytes()
    definition, errors = build_study_definition(
        decision_bytes,
        args.mapping.read_bytes(),
        args.previous_manifest.read_bytes(),
        args.previous_planning_packet.read_bytes(),
    )
    if definition is None:
        print(json.dumps({"errors": errors}, ensure_ascii=False, sort_keys=True), file=sys.stderr)
        return 2

    definition_bytes = canonical_json_bytes(definition)
    _write_immutable(args.decision_output, decision_bytes, "strategy decision")
    _write_immutable(args.definition_output, definition_bytes, "study definition")
    print(
        json.dumps(
            {
                "decision_sha256": hashlib.sha256(decision_bytes).hexdigest(),
                "definition_sha256": hashlib.sha256(definition_bytes).hexdigest(),
                "ready_for_measurement": definition["readiness"]["ready_for_measurement"],
                "decision_output": str(args.decision_output.resolve()),
                "definition_output": str(args.definition_output.resolve()),
                "decision_ref": STRATEGY_DECISION_REF,
                "definition_ref": STUDY_DEFINITION_REF,
            },
            sort_keys=True,
        )
    )
    return 0


def _write_immutable(path: Path, payload: bytes, artifact_name: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open("xb") as handle:
            handle.write(payload)
            handle.flush()
    except FileExistsError:
        if path.read_bytes() != payload:
            raise RuntimeError(f"refusing to overwrite a different {artifact_name}: {path}")


if __name__ == "__main__":
    raise SystemExit(main())
