from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.adapters.fndds_survey import (  # noqa: E402
    FNDDS_ADAPTER_VERSION,
    FNDDS_ARCHIVE_FILENAME,
    FNDDS_EXPECTED_RECORD_COUNT,
    FNDDS_RELEASE,
    FnddsSurveyAdapter,
    extract_pinned_archive_member,
)
from nutrition_data_factory.curation.fndds_review import (  # noqa: E402
    FnddsReviewError,
    build_fndds_review_report,
    load_fndds_review_policy,
    serialize_fndds_review_artifact,
)
from nutrition_data_factory.source_registry import SourceRegistry  # noqa: E402


POLICY_PATH = ROOT / "config" / "fndds-secondary-review-policy.json"
SOURCE_REGISTRY_PATH = ROOT / "config" / "source_registry.json"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Build bounded, review-only evidence from the pinned FNDDS 2021-2023 archive"
    )
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--policy", type=Path, default=POLICY_PATH)
    parser.add_argument(
        "--packet-target",
        help="write one generated candidate packet instead of the full review report",
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)

    try:
        loaded_policy = load_fndds_review_policy(args.policy)
        policy = loaded_policy["policy"]
        source = policy["source"]
        if (
            source.get("archive_filename") != FNDDS_ARCHIVE_FILENAME
            or source.get("release") != FNDDS_RELEASE
            or source.get("expected_record_count") != FNDDS_EXPECTED_RECORD_COUNT
        ):
            raise FnddsReviewError("review policy does not match the pinned FNDDS release constants")
        registry_bytes = SOURCE_REGISTRY_PATH.read_bytes()
        registry = SourceRegistry.load(SOURCE_REGISTRY_PATH)
        registry_source = registry.get(source["source_code"])
        if (
            registry_source.rights_state != "reference_only"
            or registry_source.production_eligible
            or registry_source.production_ingestion != "blocked_until_product_selection"
            or set(registry_source.allowed_uses) != {"analysis", "reference"}
            or "staged_candidate" in registry_source.allowed_uses
            or "staged_candidate" not in registry_source.prohibited_uses
        ):
            raise FnddsReviewError("FNDDS registry state no longer blocks staged or production use")
        if registry_source.rights_state != source["rights_state"]:
            raise FnddsReviewError("review policy and source registry rights states do not match")

        archive_bytes = args.archive.read_bytes()
        if source.get("archive_size_bytes") is not None and len(archive_bytes) != source["archive_size_bytes"]:
            raise FnddsReviewError("archive size does not match the pinned FNDDS release")
        member_bytes = extract_pinned_archive_member(archive_bytes, args.archive.name, source)
        if source.get("member_size_bytes") is not None and len(member_bytes) != source["member_size_bytes"]:
            raise FnddsReviewError("archive member size does not match the pinned FNDDS release")
        parsed = FnddsSurveyAdapter().parse(
            member_bytes,
            release=FNDDS_RELEASE,
            expected_sha256=source["member_sha256"],
        )
        if parsed.schema_fingerprint != source["schema_fingerprint"]:
            raise FnddsReviewError("FNDDS source schema fingerprint changed")
        if parsed.raw_record_count != source["expected_record_count"]:
            raise FnddsReviewError("FNDDS raw record count does not match the pinned release")
        if not parsed.valid or len(parsed.accepted_records) != FNDDS_EXPECTED_RECORD_COUNT:
            raise FnddsReviewError("FNDDS importer rejected one or more source records")

        evidence = {
            "archive_filename": args.archive.name,
            "archive_sha256": _sha256(archive_bytes),
            "archive_size_bytes": len(archive_bytes),
            "member_name": source["archive_member"],
            "member_sha256": parsed.source_sha256,
            "member_size_bytes": len(member_bytes),
            "schema_fingerprint": parsed.schema_fingerprint,
            "source_use_boundary": {
                "rights_state": registry_source.rights_state,
                "production_ingestion": registry_source.production_ingestion,
                "allowed_uses": list(registry_source.allowed_uses),
                "prohibited_uses": list(registry_source.prohibited_uses),
                "production_eligible": registry_source.production_eligible,
            },
        }
        report = build_fndds_review_report(parsed, loaded_policy, evidence)
        if not report["validation_report"]["passed"]:
            raise FnddsReviewError("FNDDS nutrient/source validation did not pass")
        output_document = report
        if args.packet_target is not None:
            matching_packets = [
                packet
                for packet in report["candidate_packets"]
                if packet["target_id"] == args.packet_target
            ]
            if len(matching_packets) != 1:
                raise FnddsReviewError(
                    f"expected one candidate packet for target {args.packet_target}"
                )
            output_document = matching_packets[0]
        report["source_registry_evidence"] = {
            "sha256": _sha256(registry_bytes),
            "source_code": registry_source.code,
            "status_initial": registry_source.status_initial,
            "rights_state": registry_source.rights_state,
            "production_ingestion": registry_source.production_ingestion,
            "allowed_uses": list(registry_source.allowed_uses),
            "prohibited_uses": list(registry_source.prohibited_uses),
            "production_eligible": registry_source.production_eligible,
        }
        output_bytes = serialize_fndds_review_artifact(output_document)
        output = args.output.resolve()
        output.parent.mkdir(parents=True, exist_ok=True)
        if output.exists():
            if output.read_bytes() != output_bytes:
                raise FnddsReviewError(f"refusing to overwrite a different review report: {output}")
        else:
            output.write_bytes(output_bytes)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"FNDDS secondary review failed: {error}", file=sys.stderr)
        return 2

    print(
        f"report={args.output} importer={FNDDS_ADAPTER_VERSION} "
        f"records={parsed.raw_record_count} candidates={report['counts']['candidate_packet_count']} "
        f"sha256={_sha256(output_bytes)}"
    )
    return 0


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


if __name__ == "__main__":
    raise SystemExit(main())
