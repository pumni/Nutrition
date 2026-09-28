from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.artifacts import ArtifactStore  # noqa: E402
from nutrition_data_factory.portion import (  # noqa: E402
    build_portion_review_packet,
    compile_portion_study,
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Validate a portion measurement study and write a blocked human review packet"
    )
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--measurements", type=Path)
    parser.add_argument("--artifact-store", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--reviewer", help="named human reviewer, if a durable review exists")
    parser.add_argument("--review-ref", help="durable reference to a human measurement review")
    args = parser.parse_args(argv)

    manifest_bytes = args.manifest.read_bytes()
    manifest_value = json.loads(manifest_bytes.decode("utf-8"))
    store = ArtifactStore(args.artifact_store)
    manifest_artifact = store.put_bytes(manifest_bytes, "application/json")

    measurements_value: Any = []
    measurements_sha256 = None
    measurements_artifact = None
    if args.measurements is not None:
        measurements_bytes = args.measurements.read_bytes()
        measurements_value = json.loads(measurements_bytes.decode("utf-8"))
        measurements_artifact = store.put_bytes(measurements_bytes, "application/json")
        measurements_sha256 = measurements_artifact.sha256

    compilation = compile_portion_study(
        manifest_value,
        measurements_value,
        reviewer=args.reviewer,
        reviewer_approval_ref=args.review_ref,
    )
    compiler_path = ROOT / "src" / "nutrition_data_factory" / "portion.py"
    packet = build_portion_review_packet(
        compilation,
        manifest_sha256=manifest_artifact.sha256,
        measurements_sha256=measurements_sha256,
        manifest_artifact=manifest_artifact.to_dict(),
        measurements_artifact=measurements_artifact.to_dict() if measurements_artifact else None,
        compiler_sha256=hashlib.sha256(compiler_path.read_bytes()).hexdigest(),
    )
    packet_bytes = (json.dumps(packet, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")
    packet_artifact = store.put_bytes(packet_bytes, "application/json")
    _write_immutable(args.output, packet_bytes)
    print(
        json.dumps(
            {
                "packet_sha256": packet_artifact.sha256,
                "status": packet["status"],
                "publication_status": packet["publication"]["status"],
                "output": str(args.output.resolve()),
            },
            sort_keys=True,
        )
    )
    return 0


def _write_immutable(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open("xb") as handle:
            handle.write(payload)
            handle.flush()
    except FileExistsError:
        if path.read_bytes() != payload:
            raise RuntimeError(f"refusing to overwrite a different review packet: {path}")


if __name__ == "__main__":
    raise SystemExit(main())
