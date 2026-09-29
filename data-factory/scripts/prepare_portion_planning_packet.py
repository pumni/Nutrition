from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.portion_planning import (  # noqa: E402
    build_portion_planning_packet,
    canonical_json_bytes,
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Validate reviewed food evidence and emit a blocked portion-study planning packet"
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=ROOT / "config" / "portion-study-com-trang-bat-0.3.0.json",
    )
    parser.add_argument(
        "--reviewed-mapping",
        type=Path,
        default=ROOT
        / "docs"
        / "reviews"
        / "vietnamese-basic-food-identity-issue-35-reviewed-mapping-0.1.0.json",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "docs" / "reviews" / "vietnamese-com-trang-bat-portion-planning-0.1.0.json",
    )
    args = parser.parse_args(argv)

    packet, errors = build_portion_planning_packet(
        args.manifest.read_bytes(), args.reviewed_mapping.read_bytes()
    )
    if packet is None:
        print(json.dumps({"errors": errors}, ensure_ascii=False, sort_keys=True), file=sys.stderr)
        return 2

    packet_bytes = canonical_json_bytes(packet)
    _write_immutable(args.output, packet_bytes)
    print(
        json.dumps(
            {
                "packet_sha256": hashlib.sha256(packet_bytes).hexdigest(),
                "status": packet["status"],
                "ready_for_measurement": packet["readiness"]["ready_for_measurement"],
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
            raise RuntimeError(f"refusing to overwrite a different planning packet: {path}")


if __name__ == "__main__":
    raise SystemExit(main())
