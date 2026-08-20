from __future__ import annotations

import argparse
import json
from pathlib import Path

from .pipeline import run_synthetic_pipeline


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="nutrition-data-factory")
    subparsers = parser.add_subparsers(dest="command", required=True)
    pipeline = subparsers.add_parser("pipeline", help="run the synthetic DATA-000 pipeline")
    pipeline.add_argument("--config", type=Path, required=True)
    pipeline.add_argument("--fixture", type=Path, required=True)
    pipeline.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == "pipeline":
        output = run_synthetic_pipeline(args.config, args.fixture, args.output)
        print(json.dumps({"output": str(output), "production_eligible": False}, sort_keys=True))
        return 0
    parser.error(f"unsupported command: {args.command}")
    return 2

