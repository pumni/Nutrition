from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.release.gate import build_release_gate_report  # noqa: E402


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Build a human review packet for a candidate release")
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--package-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--backend-head", help="read-only backend commit used to audit the compatibility baseline")
    parser.add_argument(
        "--compatibility-manifest",
        type=Path,
        default=ROOT / "config" / "backend-fdc-selection.json",
    )
    args = parser.parse_args(argv)
    report = build_release_gate_report(args.package_dir, args.run_dir, args.compatibility_manifest, args.backend_head)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output.resolve()), "status": report["status"]}, sort_keys=True))
    return 0 if report["status"] != "failed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
