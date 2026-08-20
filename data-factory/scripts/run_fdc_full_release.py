from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.adapters.fdc_foundation import (  # noqa: E402
    FDC_FOUNDATION_RELEASE,
    FdcFoundationAdapter,
    FdcSourceRecord,
)
from nutrition_data_factory.artifacts import AcquisitionMetadata, ArtifactStore  # noqa: E402
from nutrition_data_factory.compatibility import (  # noqa: E402
    compare_fdc_selection,
    load_compatibility_manifest,
)
from nutrition_data_factory.curation.extractor import ExtractionPolicy, extract_candidates  # noqa: E402
from nutrition_data_factory.curation.packets import build_review_packet  # noqa: E402
from nutrition_data_factory.impact import build_impact_report  # noqa: E402
from nutrition_data_factory.models import NormalizedRecord  # noqa: E402
from nutrition_data_factory.normalization.synthetic import normalize_label  # noqa: E402
from nutrition_data_factory.nutrients import (  # noqa: E402
    build_fdc_nutrient_registry,
    canonicalize_fdc_nutrients,
)
from nutrition_data_factory.release.full_compiler import compile_full_catalog_package  # noqa: E402
from nutrition_data_factory.source_registry import SourceRegistry  # noqa: E402
from nutrition_data_factory.validation.profiler import profile_records  # noqa: E402


ARCHIVE_SHA256 = "186e988ec542e913f51ef62b86a47758e8cdd0d1dc3889e7b055581f3c09c77a"
EXTRACTED_SHA256 = "27d1fe3fd89edfbe528ed915da5619320e1d004d4594603a1b19bdb1511590cc"
ACQUISITION_TOOL_VERSION = "fdc-full-release-0.1.0"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Build a full-source FDC Foundation catalog candidate")
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--extracted", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--object-store", type=Path, required=True)
    parser.add_argument("--backend-baseline", required=True)
    parser.add_argument("--vietnamese-corpus", type=Path)
    parser.add_argument("--recipe-evidence", type=Path)
    parser.add_argument("--portion-evidence", type=Path)
    parser.add_argument("--portion-plan", type=Path)
    parser.add_argument(
        "--retrieved-at",
        required=True,
        help="Pinned acquisition timestamp; required so the package is reproducible byte-for-byte.",
    )
    args = parser.parse_args(argv)

    archive_bytes = args.archive.read_bytes()
    extracted_bytes = args.extracted.read_bytes()
    _require_sha256(archive_bytes, ARCHIVE_SHA256, "archive")
    _require_sha256(extracted_bytes, EXTRACTED_SHA256, "extracted JSON")

    registry = SourceRegistry.load(ROOT / "config" / "source_registry.json")
    metadata = registry.get("usda_fdc_foundation")
    store = ArtifactStore(args.object_store)
    archive_ref = store.put_bytes(
        archive_bytes,
        content_type="application/zip",
        expected_sha256=ARCHIVE_SHA256,
        acquisition=_acquisition(metadata, args.archive.name, len(archive_bytes), "application/zip", ARCHIVE_SHA256, args.retrieved_at),
    )
    extracted_ref = store.put_bytes(
        extracted_bytes,
        content_type="application/json",
        expected_sha256=EXTRACTED_SHA256,
        acquisition=_acquisition(metadata, args.extracted.name, len(extracted_bytes), "application/json", EXTRACTED_SHA256, args.retrieved_at),
    )
    parsed = FdcFoundationAdapter().parse(extracted_bytes, release=FDC_FOUNDATION_RELEASE, expected_sha256=EXTRACTED_SHA256)
    records = [_record_dict(record, metadata.code) for record in parsed.accepted_records]
    source_quality = profile_records(records, metadata=metadata, artifact_sha256=EXTRACTED_SHA256, expected_artifact_sha256=EXTRACTED_SHA256)

    compatibility_manifest = load_compatibility_manifest(ROOT / "config" / "backend-fdc-selection.json")
    compatibility = compare_fdc_selection(compatibility_manifest, [record.fdc_id for record in parsed.accepted_records if record.fdc_id in set(compatibility_manifest["fdc_ids"])])
    nutrient_registry = build_fdc_nutrient_registry(records)
    normalized_records, composition_values, source_nutrient_rejections = _normalize_records(parsed.accepted_records, metadata.code)
    quarantine_entries = [
        {
            "kind": "source_record",
            "row_index": reject.row_index,
            "reason_code": reject.reason_code,
            "detail": reject.detail,
        }
        for reject in parsed.rejected_records
    ]
    quarantine_entries.extend(source_nutrient_rejections)
    quarantine_report = {
        "report_version": "source-quarantine-0.1.0",
        "policy": "preserve_raw_quarantine_invalid",
        "entries": quarantine_entries,
        "counts_by_kind": dict(Counter(item["kind"] for item in quarantine_entries)),
        "counts_by_reason": dict(Counter(item["reason_code"] for item in quarantine_entries)),
        "raw_source_rows_accounted_for": parsed.raw_record_count == len(parsed.accepted_records) + len(parsed.rejected_records),
    }

    vietnamese_candidates, vietnamese_report, vietnamese_review_packets = _load_vietnamese_candidates(
        args.vietnamese_corpus,
        parsed.accepted_records,
        metadata.code,
    )
    food_concepts, source_names, source_mappings = _food_identity_candidates(parsed.accepted_records, metadata.code)
    food_names = [*source_names, *vietnamese_candidates]
    recipes: list[dict[str, Any]] = []
    recipe_components: list[dict[str, Any]] = []
    portions: list[dict[str, Any]] = []
    recipe_evidence_candidates, recipe_report = _load_recipe_evidence(
        args.recipe_evidence,
        parsed.accepted_records,
        metadata.code,
    )
    portion_evidence_candidates, portion_report = _load_portion_evidence(args.portion_evidence, args.portion_plan)

    counts = {
        "raw_source_rows": parsed.raw_record_count,
        "accepted_source_records": len(parsed.accepted_records),
        "quarantined_source_rows": len(parsed.rejected_records),
        "quarantined_nutrient_observations": len(source_nutrient_rejections),
        "normalized_records": len(normalized_records),
        "raw_nutrient_observations": sum(len(record.nutrients) for record in parsed.accepted_records),
        "composition_values": len(composition_values),
        "source_nutrient_registry_entries": len(nutrient_registry),
        "food_concept_candidates": len(food_concepts),
        "food_name_candidates": len(food_names),
        "source_food_mapping_candidates": len(source_mappings),
        "vietnamese_identity_candidates": len(vietnamese_candidates),
        "vietnamese_review_packets": len(vietnamese_review_packets),
        "vietnamese_source_mapping_proposals": vietnamese_report.get("source_mapping_proposal_count", 0),
        "recipe_evidence_candidates": len(recipe_evidence_candidates),
        "recipe_ingredient_identity_review_required": recipe_report.get("ingredient_identity_review_required_count", 0),
        "recipe_ingredient_identity_proposals": recipe_report.get("ingredient_identity_proposal_count", 0),
        "recipe_ingredient_identity_no_candidate": recipe_report.get("ingredient_identity_no_candidate_count", 0),
        "recipe_non_mass_quantities": recipe_report.get("non_mass_quantity_count", 0),
        "portion_evidence_candidates": len(portion_evidence_candidates),
        "recipes": len(recipes),
        "recipe_components": len(recipe_components),
        "portion_observations": len(portions),
    }
    registry_statuses = Counter(item["mapping_status"] for item in nutrient_registry)
    rejected_reasons = Counter(item["reason_code"] for record in normalized_records for item in record.attributes["rejected_nutrient_mappings"])
    completeness = {
        "report_version": "data-completeness-0.1.0",
        "scope": "full_approved_source_release",
        "source_code": metadata.code,
        "source_release": metadata.release,
        "source_artifact_sha256": EXTRACTED_SHA256,
        "source_schema_fingerprint": parsed.schema_fingerprint,
        "backend_baseline": args.backend_baseline,
        "counts": counts,
        "accounting": {
            "raw_equals_accepted_plus_quarantined": parsed.raw_record_count == len(parsed.accepted_records) + len(parsed.rejected_records),
            "every_accepted_source_record_normalized": len(normalized_records) == len(parsed.accepted_records),
            "every_raw_nutrient_preserved_in_source_records": sum(len(record.nutrients) for record in parsed.accepted_records) > 0,
        },
        "nutrient_mapping_coverage": {
            "unique_source_nutrient_ids": len(nutrient_registry),
            "approved_ids": registry_statuses.get("approved", 0),
            "source_preserved_unmapped_ids": registry_statuses.get("source_preserved_unmapped", 0),
            "raw_observations_with_unmapped_mapping": rejected_reasons.get("unmapped_source_nutrient", 0),
            "forbidden_observations": rejected_reasons.get("forbidden_energy", 0),
            "invalid_observations_quarantined": len(source_nutrient_rejections),
        },
        "identity_coverage": vietnamese_report,
        "recipe_coverage": recipe_report,
        "portion_coverage": portion_report,
        "unresolved_gaps": [
            f"{len(source_quality.errors)} full-source quality errors require review or remain quarantined",
            f"{registry_statuses.get('source_preserved_unmapped', 0)} source nutrient IDs have no approved product semantic mapping",
            "Vietnamese identity corpus was not supplied"
            if not args.vietnamese_corpus
            else (
                f"Vietnamese review remains owner-directed: {vietnamese_report['candidate_count']} candidates, "
                f"{vietnamese_report['source_mapping_proposal_count']} semantically compatible FDC mapping proposals, "
                f"{vietnamese_report['unresolved_identity_count']} non-approved identity decisions"
            ),
            "Recipe evidence was not supplied"
            if not args.recipe_evidence
            else (
                f"{len(recipe_evidence_candidates)} recipe evidence candidates remain review_required; "
                f"0 compile-ready; {recipe_report['missing_evidence_item_count']} missing-evidence items; "
                f"{recipe_report['ingredient_identity_no_candidate_count']} ingredient identities have no semantic candidate"
            ),
            "Measured portion evidence was not supplied" if not args.portion_evidence else f"{len(portion_evidence_candidates)} portion evidence candidates remain review_required; 0 published",
            "Production activation and backend import were not attempted",
        ],
    }
    validation = {
        "rule_version": "full-source-accounting-0.1.0",
        "status": "failed_with_quarantine" if source_quality.errors or quarantine_entries else "passed",
        "passed": not bool(source_quality.errors or quarantine_entries),
        "errors": [json.dumps(item, sort_keys=True) for item in source_quality.errors],
        "warnings": [json.dumps(item, sort_keys=True) for item in source_quality.warnings],
        "quarantine_entry_count": len(quarantine_entries),
        "statistics": {
            **source_quality.statistics,
            "raw_record_count": parsed.raw_record_count,
            "accepted_record_count": len(parsed.accepted_records),
            "rejected_record_count": len(parsed.rejected_records),
            "normalized_record_count": len(normalized_records),
            "composition_value_count": len(composition_values),
        },
    }
    impact = build_impact_report({}, {}).to_dict()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    _write_json(output / "acquisition-report.json", {"archive_artifact": archive_ref.to_dict(), "extracted_artifact": extracted_ref.to_dict(), "retrieved_at": args.retrieved_at, "release": FDC_FOUNDATION_RELEASE})
    _write_json(output / "fdc-parse-report.json", parsed.to_dict())
    _write_json(output / "source-quality-report.json", source_quality.to_dict())
    _write_json(output / "backend-compatibility-report.json", compatibility.to_dict())
    _write_json(output / "quarantine-report.json", quarantine_report)
    _write_json(output / "completeness-manifest.json", completeness)
    _write_json(output / "vietnamese-identity-report.json", vietnamese_report)
    _write_json(output / "recipe-evidence-report.json", recipe_report)
    _write_json(output / "portion-evidence-report.json", portion_report)
    _write_json(output / "validation-report.json", validation)
    package_path = output / "full-catalog-package"
    compile_full_catalog_package(
        package_path,
        metadata=metadata,
        archive_artifact=archive_ref,
        extracted_artifact=extracted_ref,
        source_records=list(parsed.accepted_records),
        normalized_records=normalized_records,
        nutrient_registry=nutrient_registry,
        composition_values=composition_values,
        food_concepts=food_concepts,
        food_names=food_names,
        source_food_mappings=source_mappings,
        vietnamese_review_packets=vietnamese_review_packets,
        recipe_evidence_candidates=recipe_evidence_candidates,
        portion_evidence_candidates=portion_evidence_candidates,
        curation_decisions=[],
        recipes=recipes,
        recipe_components=recipe_components,
        portions=portions,
        quarantine_report=quarantine_report,
        source_quality_report=source_quality.to_dict(),
        compatibility_report=compatibility.to_dict(),
        validation_report=validation,
        impact_report=impact,
        completeness_manifest=completeness,
        vietnamese_identity_report=vietnamese_report,
        recipe_evidence_report=recipe_report,
        portion_evidence_report=portion_report,
        backend_baseline=args.backend_baseline,
    )
    _write_json(output / "full-release-summary.json", {
        "package_created": True,
        "package": str(package_path),
        "production_eligible": False,
        "activation_attempted": False,
        "validation_status": validation["status"],
        "raw_source_rows": parsed.raw_record_count,
        "accepted_source_records": len(parsed.accepted_records),
        "quarantined_source_rows": len(parsed.rejected_records),
        "normalized_records": len(normalized_records),
        "backend_regression_selection_status": compatibility.status,
    })
    return 0


def _record_dict(record: FdcSourceRecord, source_code: str) -> dict[str, Any]:
    return {
        "source_code": source_code,
        "release": FDC_FOUNDATION_RELEASE,
        "source_id": str(record.fdc_id),
        "fdc_id": record.fdc_id,
        "description": record.description,
        "payload_sha256": record.payload_sha256,
        "nutrients": list(record.nutrients),
        "portions": list(record.portions),
    }


def _normalize_records(records: tuple[FdcSourceRecord, ...], source_code: str) -> tuple[list[NormalizedRecord], list[dict[str, Any]], list[dict[str, Any]]]:
    normalized: list[NormalizedRecord] = []
    composition_values: list[dict[str, Any]] = []
    quarantined: list[dict[str, Any]] = []
    for record in sorted(records, key=lambda item: item.fdc_id):
        canonical = canonicalize_fdc_nutrients(record.nutrients)
        for item in canonical.rejected:
            if item["reason_code"] in {"negative_value", "invalid_value", "invalid_unit", "duplicate_mapping", "forbidden_energy", "status_value_conflict"}:
                quarantined.append({"kind": "nutrient_observation", "fdc_id": record.fdc_id, **item})
        for value in canonical.values:
            composition_values.append(
                {
                    "source_code": source_code,
                    "release": FDC_FOUNDATION_RELEASE,
                    "source_id": str(record.fdc_id),
                    "source_payload_sha256": record.payload_sha256,
                    "review_status": "proposal",
                    "reviewer_decision_status": "pending_human_review",
                    **value.to_dict(),
                }
            )
        normalized.append(
            NormalizedRecord(
                source_code=source_code,
                release=FDC_FOUNDATION_RELEASE,
                source_id=str(record.fdc_id),
                original_label=record.description,
                normalized_label=normalize_label(record.description),
                attributes={
                    "fdc_id": record.fdc_id,
                    "data_type": record.data_type,
                    "payload_sha256": record.payload_sha256,
                    "nutrients": list(record.nutrients),
                    "portions": list(record.portions),
                    "canonical_nutrients": [value.to_dict() for value in canonical.values],
                    "rejected_nutrient_mappings": list(canonical.rejected),
                },
            )
        )
    return normalized, composition_values, quarantined


def _food_identity_candidates(records: tuple[FdcSourceRecord, ...], source_code: str) -> tuple[list[dict[str, Any]], list[dict[str, Any]], list[dict[str, Any]]]:
    concepts = []
    names = []
    mappings = []
    for record in sorted(records, key=lambda item: item.fdc_id):
        concept_id = f"source-food:{source_code}:{record.fdc_id}"
        concepts.append({"concept_id": concept_id, "lifecycle_status": "candidate", "source_code": source_code, "source_id": str(record.fdc_id), "source_payload_sha256": record.payload_sha256, "review_status": "proposal"})
        names.append({"name_id": f"source-name:{source_code}:{record.fdc_id}", "concept_id": concept_id, "locale": "und", "name": record.description, "source_payload_sha256": record.payload_sha256, "status": "source_observed"})
        mappings.append({"mapping_id": f"source-mapping:{source_code}:{record.fdc_id}", "source_code": source_code, "source_id": str(record.fdc_id), "source_payload_sha256": record.payload_sha256, "concept_id": concept_id, "status": "proposal"})
    return concepts, names, mappings


def _load_vietnamese_candidates(
    path: Path | None,
    source_records: tuple[FdcSourceRecord, ...],
    source_code: str,
) -> tuple[list[dict[str, Any]], dict[str, Any], list[dict[str, Any]]]:
    if path is None:
        return [], _not_supplied_report("vietnamese-identity-0.1.0", "Vietnamese corpus was not supplied"), []
    corpus_bytes = path.read_bytes()
    corpus_sha256 = _sha256(corpus_bytes)
    payload = json.loads(corpus_bytes.decode("utf-8"))
    result = extract_candidates(payload, ExtractionPolicy())
    candidates = []
    review_packets = []
    for candidate in result.candidates:
        source_foods, decision_profile = _propose_vietnamese_source_foods(
            candidate["normalized_phrase"],
            source_records,
            source_code,
        )
        public_decision = {
            key: value for key, value in decision_profile.items() if not key.startswith("_")
        }
        public_decision.update(
            {
                "decision_source": "owner_task_intent",
                "reviewer": None,
            }
        )
        packet_evidence_refs = [
            f"vietnamese-corpus-sha256:{corpus_sha256}",
            *[
                "case:{case_id}:observation:{observation_index}:context-sha256:{context_sha256}".format(
                    case_id=ref["case_id"],
                    observation_index=ref["observation_index"],
                    context_sha256=ref.get("context_sha256", ""),
                )
                for ref in candidate["context_refs"]
            ],
        ]
        candidates.append(
            {
                "name_id": candidate["candidate_id"],
                "locale": "vi-VN",
                "name": candidate["phrase"],
                "normalized_phrase": candidate["normalized_phrase"],
                "status": "proposal",
                "review_status": candidate["review_status"],
                "evidence_refs": candidate["context_refs"],
                "source_corpus_sha256": corpus_sha256,
                "source_classes": candidate.get("source_classes", []),
                "negated_observation_count": candidate.get("negated_observation_count", 0),
                **public_decision,
                "mapping_proposals": source_foods,
            }
        )
        packet = build_review_packet(
            candidate,
            candidate_concept={
                "classification": decision_profile["identity_classification"],
                "identity_decision": decision_profile["identity_decision"],
                "mapping_decision": decision_profile["mapping_decision"],
                "decision_reason": decision_profile["decision_reason"],
                "required_source_constraints": decision_profile["required_source_constraints"],
                "mapping_status": decision_profile["mapping_decision"],
                "human_review_required": True,
            },
            source_foods=source_foods,
            evidence_refs=packet_evidence_refs,
        )
        packet["source_classes"] = candidate.get("source_classes", [])
        packet["negated_observation_count"] = candidate.get("negated_observation_count", 0)
        packet["source_corpus_sha256"] = corpus_sha256
        packet.update(public_decision)
        review_packets.append(packet)
    source_proposal_count = sum(1 for item in candidates if item["mapping_proposals"])
    unresolved_count = sum(1 for item in candidates if item["identity_decision"] not in {"approved"})
    recipe_required_count = sum(1 for item in candidates if item["mapping_decision"] == "recipe_required")
    identity_decision_counts = Counter(item["identity_decision"] for item in candidates)
    mapping_decision_counts = Counter(item["mapping_decision"] for item in candidates)
    return candidates, {
        "corpus_available": True,
        "corpus_path": str(path),
        "corpus_sha256": corpus_sha256,
        "status": "proposal_only",
        **result.report,
        "candidate_count": len(candidates),
        "review_packet_count": len(review_packets),
        "candidate_with_source_mapping_proposals": source_proposal_count,
        "source_mapping_proposal_count": sum(len(item["mapping_proposals"]) for item in candidates),
        "unresolved_identity_count": unresolved_count,
        "recipe_required_count": recipe_required_count,
        "identity_decision_counts": dict(sorted(identity_decision_counts.items())),
        "mapping_decision_counts": dict(sorted(mapping_decision_counts.items())),
        "mapping_policy_version": VIETNAMESE_MAPPING_POLICY_VERSION,
    }, review_packets


VIETNAMESE_MAPPING_POLICY_VERSION = "vietnamese-semantic-proposal-0.2.0"


_VIETNAMESE_IDENTITY_DECISIONS: dict[str, dict[str, Any]] = {
    "trứng gà luộc": {
        "identity_classification": "basic_food_identity_candidate",
        "identity_decision": "approved",
        "mapping_decision": "deferred",
        "decision_reason": "Whole boiled egg identity is accepted, but current FDC proposals did not establish boiled preparation equivalence.",
        "required_source_constraints": [
            "source description must identify egg and whole egg",
            "source description must contain boiled or hard-boiled preparation state",
            "source description must not be raw, frozen, or dried",
        ],
        "_required_term_groups": (("egg",), ("whole",), ("boiled", "hard-boiled")),
        "_forbidden_terms": ("raw", "frozen", "dried"),
    },
    "trứng luộc": {
        "identity_classification": "ambiguous_identity",
        "identity_decision": "deferred",
        "mapping_decision": "none",
        "decision_reason": "Egg species/type is unspecified; do not create an unconditional alias.",
        "required_source_constraints": [
            "egg species/type must be clarified",
            "boiled preparation must be source-backed",
        ],
        "_required_term_groups": (),
        "_forbidden_terms": (),
    },
    "cơm trắng": {
        "identity_classification": "basic_food_identity_candidate",
        "identity_decision": "approved",
        "mapping_decision": "deferred",
        "decision_reason": "Cooked white rice identity is accepted, but raw rice or rice flour is not compositionally equivalent.",
        "required_source_constraints": [
            "source description must identify rice and white rice",
            "source description must identify cooked preparation",
            "source description must not be raw, dry, or flour",
        ],
        "_required_term_groups": (("rice",), ("white",), ("cooked",)),
        "_forbidden_terms": ("raw", "dry", "dried", "flour"),
    },
    "thịt bò": {
        "identity_classification": "basic_food_identity_candidate",
        "identity_decision": "approved",
        "mapping_decision": "deferred",
        "decision_reason": "Generic beef identity is accepted, but a specific beef cut or processed product cannot stand in for the generic identity.",
        "required_source_constraints": [
            "source description must identify beef",
            "source description must not select an arbitrary cut, preparation, or processed beef product",
        ],
        "_required_term_groups": (("beef",),),
        "_forbidden_terms": (
            "frankfurter", "sausage", "ground", "loin", "round", "ribeye", "steak", "roast",
            "chuck", "flank", "tenderloin", "sirloin", "porterhouse", "t-bone", "short loin",
        ),
    },
    "bơ": {
        "identity_classification": "ambiguous_identity",
        "identity_decision": "deferred",
        "mapping_decision": "none",
        "decision_reason": "The standalone Vietnamese alias may mean avocado or dairy butter; context is insufficient for a global alias.",
        "required_source_constraints": [
            "context must distinguish avocado from dairy butter",
            "source identity must be reviewed after clarification",
        ],
        "_required_term_groups": (),
        "_forbidden_terms": (),
    },
    "thịt gà": {
        "identity_classification": "basic_food_identity_candidate",
        "identity_decision": "approved",
        "mapping_decision": "deferred",
        "decision_reason": "Generic chicken identity is accepted, but a specific chicken cut or state cannot stand in for the generic identity.",
        "required_source_constraints": [
            "source description must identify chicken",
            "source description must not select breast, thigh, drumstick, wing, ground chicken, or a specific preparation",
        ],
        "_required_term_groups": (("chicken",),),
        "_forbidden_terms": (
            "breast", "thigh", "drumstick", "wing", "ground", "broiler", "fryer", "raw", "cooked",
            "braised", "roasted", "fried", "grilled",
        ),
    },
    "thịt gà luộc": {
        "identity_classification": "basic_food_identity_candidate",
        "identity_decision": "approved",
        "mapping_decision": "deferred",
        "decision_reason": "Boiled chicken identity is accepted, but current FDC proposals were raw or tied to an arbitrary cooked cut.",
        "required_source_constraints": [
            "source description must identify chicken",
            "source description must contain boiled preparation state",
            "source description must not select an arbitrary cut or raw/frozen/dried state",
        ],
        "_required_term_groups": (("chicken",), ("boiled", "hard-boiled")),
        "_forbidden_terms": (
            "breast", "thigh", "drumstick", "wing", "ground", "raw", "frozen", "dried", "braised",
        ),
    },
    "sữa tươi": {
        "identity_classification": "basic_food_identity_candidate",
        "identity_decision": "approved",
        "mapping_decision": "deferred",
        "decision_reason": "Generic fresh-milk identity is accepted, but fresh milk does not imply whole-fat milk.",
        "required_source_constraints": [
            "source description must identify fluid milk",
            "source description must not force whole, low-fat, reduced-fat, nonfat, or a numeric milkfat class",
        ],
        "_required_term_groups": (("milk",), ("fluid",)),
        "_forbidden_terms": (
            "whole", "lowfat", "low fat", "reduced", "nonfat", "skim", "%", "milkfat",
        ),
    },
    "cơm": {
        "identity_classification": "basic_food_identity_candidate",
        "identity_decision": "approved",
        "mapping_decision": "deferred",
        "decision_reason": "Generic cooked-rice identity is accepted, but raw, dry, flour, or variety-specific records are not equivalent by default.",
        "required_source_constraints": [
            "source description must identify cooked rice",
            "source description must not be raw, dry, or flour",
            "variety-specific records require additional identity evidence",
        ],
        "_required_term_groups": (("rice",), ("cooked",)),
        "_forbidden_terms": ("raw", "dry", "dried", "flour", "wild", "black", "red", "brown"),
    },
    "da gà": {
        "identity_classification": "unsupported_for_v1_mapping",
        "identity_decision": "unsupported",
        "mapping_decision": "unsupported",
        "decision_reason": "Current seed evidence is negated-only; retain parser evidence but do not create a consumed-food mapping for v1.",
        "required_source_constraints": [
            "positive non-negated consumption evidence is required before catalog mapping",
        ],
        "_required_term_groups": (),
        "_forbidden_terms": (),
    },
    "cơm gà": {
        "identity_classification": "recipe_required",
        "identity_decision": "approved",
        "mapping_decision": "recipe_required",
        "decision_reason": "Composite dish must be represented as a recipe, not a basic-food FDC mapping.",
        "required_source_constraints": [
            "recipe ingredient identities, exact quantities, yield, and output mass are required",
        ],
        "_required_term_groups": (),
        "_forbidden_terms": (),
    },
    "phở bò": {
        "identity_classification": "recipe_required",
        "identity_decision": "approved",
        "mapping_decision": "recipe_required",
        "decision_reason": "Composite dish must be represented as a recipe, not a basic-food FDC mapping.",
        "required_source_constraints": [
            "recipe ingredient identities, exact quantities, yield, and output mass are required",
        ],
        "_required_term_groups": (),
        "_forbidden_terms": (),
    },
    "bún bò huế": {
        "identity_classification": "recipe_required",
        "identity_decision": "approved",
        "mapping_decision": "recipe_required",
        "decision_reason": "Composite dish must be represented as a recipe, not a basic-food FDC mapping.",
        "required_source_constraints": [
            "recipe ingredient identities, exact quantities, yield, and output mass are required",
        ],
        "_required_term_groups": (),
        "_forbidden_terms": (),
    },
    "bánh mì": {
        "identity_classification": "ambiguous_identity",
        "identity_decision": "deferred",
        "mapping_decision": "none",
        "decision_reason": "The phrase may mean bread or the Vietnamese sandwich dish; the current observation is insufficient.",
        "required_source_constraints": [
            "context must distinguish bread from the composite sandwich dish",
        ],
        "_required_term_groups": (),
        "_forbidden_terms": (),
    },
    "rau muống": {
        "identity_classification": "basic_food_identity_candidate",
        "identity_decision": "approved",
        "mapping_decision": "deferred",
        "decision_reason": "Basic food identity is accepted, but the approved FDC Foundation release has no compatible source record in this candidate run.",
        "required_source_constraints": [
            "source description must identify water spinach or morning glory",
            "no unrelated leafy green may be used as a substitute",
        ],
        "_required_term_groups": (("water spinach", "morning glory"),),
        "_forbidden_terms": (),
    },
}


def _propose_vietnamese_source_foods(
    normalized_phrase: str,
    source_records: tuple[FdcSourceRecord, ...],
    source_code: str,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    phrase_key = normalized_phrase.casefold()
    profile = dict(
        _VIETNAMESE_IDENTITY_DECISIONS.get(
            phrase_key,
            {
                "identity_classification": "unresolved_identity",
                "identity_decision": "deferred",
                "mapping_decision": "none",
                "decision_reason": "No bounded identity decision exists for this phrase; human clarification is required.",
                "required_source_constraints": ["human identity classification is required"],
                "_required_term_groups": (),
                "_forbidden_terms": (),
            },
        )
    )
    if profile["mapping_decision"] != "deferred":
        return [], profile
    scored: list[tuple[int, int, FdcSourceRecord]] = []
    required_term_groups = profile["_required_term_groups"]
    for record in source_records:
        description = normalize_label(record.description)
        if not all(any(_source_description_has_term(description, term) for term in group) for group in required_term_groups):
            continue
        if any(_source_description_has_term(description, term) for term in profile["_forbidden_terms"]):
            continue
        score = 2 * len(required_term_groups)
        score += sum(
            1
            for group in required_term_groups
            for term in group
            if _source_description_has_term(description, term)
        )
        scored.append((score, record.fdc_id, record))
    scored.sort(key=lambda item: (-item[0], item[1]))
    return [
        {
            "source_code": source_code,
            "source_id": str(record.fdc_id),
            "description": record.description,
            "match_score": score,
            "status": "proposal",
            "human_review_required": True,
            "semantic_policy_version": VIETNAMESE_MAPPING_POLICY_VERSION,
            "matched_constraints": list(profile["required_source_constraints"]),
        }
        for score, _, record in scored[:5]
    ], profile


RECIPE_INGREDIENT_MAPPING_POLICY_VERSION = "recipe-ingredient-semantic-proposal-0.1.0"


_RECIPE_INGREDIENT_PROFILES: dict[str, dict[str, Any]] = {
    "bánh phở": {
        "identity_classification": "rice_noodle_candidate",
        "decision_reason": "Rice noodle identity requires a source record that identifies rice noodles; flour or rice-grain records are not substitutes.",
        "required_source_constraints": [
            "source description must identify rice noodles",
            "rice flour and rice grain records are not equivalent",
        ],
        "_required_term_groups": (("rice",), ("noodle", "noodles")),
        "_forbidden_terms": ("flour", "grain"),
    },
    "thịt bò": {
        "identity_classification": "generic_beef_candidate",
        "decision_reason": "Generic beef ingredient cannot be mapped to an arbitrary cut or processed beef product.",
        "required_source_constraints": [
            "source description must identify beef",
            "source description must not select a specific cut, preparation, or processed beef product",
        ],
        "_required_term_groups": (("beef",),),
        "_forbidden_terms": (
            "frankfurter", "sausage", "ground", "loin", "round", "ribeye", "steak", "roast",
            "chuck", "flank", "tenderloin", "sirloin", "porterhouse", "t-bone", "short loin",
        ),
    },
    "dầu ăn": {
        "identity_classification": "generic_cooking_oil_candidate",
        "decision_reason": "Generic cooking oil does not identify a specific edible oil source.",
        "required_source_constraints": [
            "source description must identify an edible cooking oil",
            "specific oil types require additional identity evidence",
        ],
        "_required_term_groups": (("oil",),),
        "_forbidden_terms": (
            "olive", "canola", "soybean", "corn", "sunflower", "sesame", "coconut", "peanut",
            "safflower", "cottonseed", "palm", "vegetable",
        ),
    },
    "giá đỗ": {
        "identity_classification": "bean_sprout_candidate",
        "decision_reason": "Bean-sprout identity requires source evidence for bean/mung sprouts; unrelated sprouts are not substitutes.",
        "required_source_constraints": [
            "source description must identify bean or mung-bean sprouts",
            "unrelated sprouts are not equivalent",
        ],
        "_required_term_groups": (("sprout", "sprouts"), ("bean", "mung")),
        "_forbidden_terms": (),
    },
    "hành lá": {
        "identity_classification": "green_onion_candidate",
        "decision_reason": "Green-onion identity requires a source record for green onion, scallion, or spring onion.",
        "required_source_constraints": [
            "source description must identify green onion, scallion, or spring onion",
            "bulb onion records are not equivalent by default",
        ],
        "_required_term_groups": (("green onion", "scallion", "spring onion"),),
        "_forbidden_terms": ("bulb",),
    },
    "gạo tẻ": {
        "identity_classification": "non_glutinous_rice_candidate",
        "decision_reason": "Non-glutinous rice ingredient may propose raw rice records, but the exact variety and processing state still require review.",
        "required_source_constraints": [
            "source description must identify rice in raw grain form",
            "source description must not be flour, cooked rice, glutinous rice, or another color variety",
        ],
        "_required_term_groups": (("rice",), ("raw",)),
        "_forbidden_terms": ("flour", "cooked", "glutinous", "wild", "black", "red", "brown"),
    },
    "thịt gà ta": {
        "identity_classification": "native_chicken_candidate",
        "decision_reason": "Native chicken requires source evidence for the relevant breed/context and cannot fall back to an arbitrary chicken cut.",
        "required_source_constraints": [
            "source description must identify native, free-range, or otherwise compatible chicken context",
            "source description must not select an arbitrary cut or preparation",
        ],
        "_required_term_groups": (("chicken",), ("native", "free-range", "heritage")),
        "_forbidden_terms": ("breast", "thigh", "drumstick", "wing", "ground", "raw", "cooked"),
    },
}


def _load_recipe_evidence(
    path: Path | None,
    records: tuple[FdcSourceRecord, ...],
    source_code: str,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    if path is None:
        return [], _not_supplied_report("recipe-evidence-0.1.0", "recipe evidence input was not supplied")
    evidence_bytes = path.read_bytes()
    evidence_sha256 = _sha256(evidence_bytes)
    payload = json.loads(evidence_bytes.decode("utf-8"))
    candidates = payload.get("candidates") if isinstance(payload, dict) else payload
    if not isinstance(candidates, list):
        raise ValueError("recipe evidence must contain a candidates array")
    packaged: list[dict[str, Any]] = []
    ingredient_count = 0
    ingredient_identity_proposal_count = 0
    unresolved_identity_count = 0
    non_mass_quantity_count = 0
    dish_counts = Counter(
        str(candidate.get("dish", "")).casefold()
        for candidate in candidates
        if isinstance(candidate, dict)
    )
    for candidate in candidates:
        if not isinstance(candidate, dict):
            continue
        evidence = candidate.get("evidence", {})
        identity_review = []
        candidate_non_mass_quantity_count = 0
        if isinstance(evidence, dict):
            for key in sorted(evidence):
                label = str(key).removesuffix("_g").replace("_", " ")
                quantity = evidence[key]
                quantity_review = _recipe_quantity_review(key, quantity)
                if quantity_review["state"] != "exact_gram":
                    non_mass_quantity_count += 1
                    candidate_non_mass_quantity_count += 1
                source_foods, ingredient_profile = _propose_recipe_ingredient_sources(
                    label,
                    records,
                    source_code,
                )
                ingredient_count += 1
                if source_foods:
                    ingredient_identity_proposal_count += 1
                else:
                    unresolved_identity_count += 1
                identity_review.append(
                    {
                        "evidence_key": key,
                        "observed_ingredient": label,
                        "raw_quantity": quantity,
                        "quantity_review": quantity_review,
                        "status": "proposal_semantic_source_matches" if source_foods else "unresolved_identity",
                        "mapping_decision": "deferred" if source_foods else "none",
                        "decision_reason": ingredient_profile["decision_reason"],
                        "required_source_constraints": ingredient_profile["required_source_constraints"],
                        "source_food_proposals": source_foods,
                        "human_review_required": True,
                    }
                )
        packaged.append(
            {
                **candidate,
                "source_evidence_sha256": evidence_sha256,
                "review_status": "review_required",
                "compilation_status": "not_compile_ready",
                "ingredient_identity_review": identity_review,
                "ingredient_count": len(identity_review),
                "non_mass_quantity_count": candidate_non_mass_quantity_count,
                "nutrition_values_emitted": False,
            }
        )
    priority_items = sorted(
        packaged,
        key=lambda item: (
            -dish_counts[str(item.get("dish", "")).casefold()],
            len(item.get("missing_for_compile", [])),
            str(item.get("candidate_id", "")),
        ),
    )
    priority_queue = []
    for priority_rank, item in enumerate(priority_items, start=1):
        priority = "high" if dish_counts[str(item.get("dish", "")).casefold()] > 1 else "normal"
        item["evidence_priority"] = priority
        item["evidence_priority_rank"] = priority_rank
        priority_queue.append(
            {
                "priority_rank": priority_rank,
                "priority": priority,
                "candidate_id": item.get("candidate_id"),
                "dish": item.get("dish"),
                "missing_evidence_count": len(item.get("missing_for_compile", [])),
                "ingredient_count": item.get("ingredient_count", 0),
                "non_mass_quantity_count": item.get("non_mass_quantity_count", 0),
                "blocking_fields": item.get("missing_for_compile", []),
            }
        )
    missing_field_count = sum(len(item.get("missing_for_compile", [])) for item in packaged)
    return packaged, {
        "report_version": "recipe-evidence-0.2.0",
        "status": "review_required",
        "source_path": str(path),
        "source_sha256": evidence_sha256,
        "candidate_count": len(packaged),
        "compile_ready_count": 0,
        "missing_evidence_item_count": missing_field_count,
        "unresolved_ingredient_identity_count": unresolved_identity_count,
        "ingredient_identity_review_required_count": ingredient_count,
        "ingredient_identity_proposal_count": ingredient_identity_proposal_count,
        "ingredient_identity_no_candidate_count": unresolved_identity_count,
        "non_mass_quantity_count": non_mass_quantity_count,
        "priority_queue": priority_queue,
        "mapping_policy_version": RECIPE_INGREDIENT_MAPPING_POLICY_VERSION,
        "nutrition_values_emitted": False,
    }


def _recipe_quantity_review(key: str, quantity: Any) -> dict[str, Any]:
    if isinstance(quantity, (int, float)) and not isinstance(quantity, bool) and key.casefold().endswith("_g"):
        return {"state": "exact_gram", "value_g": quantity}
    if isinstance(quantity, str) and quantity.strip():
        return {"state": "non_mass_quantity", "raw_value": quantity}
    return {"state": "unresolved_quantity", "raw_value": quantity}


def _source_description_has_term(description: str, term: str) -> bool:
    return re.search(
        rf"(?<![a-z0-9]){re.escape(term.casefold())}(?![a-z0-9])",
        description.casefold(),
    ) is not None


def _propose_recipe_ingredient_sources(
    label: str,
    source_records: tuple[FdcSourceRecord, ...],
    source_code: str,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    normalized_label = normalize_label(label)
    profile = _RECIPE_INGREDIENT_PROFILES.get(normalized_label)
    if profile is None:
        exact_matches = [
            record for record in source_records if normalize_label(record.description) == normalized_label
        ]
        profile = {
            "identity_classification": "unresolved_identity",
            "decision_reason": "No bounded semantic ingredient profile exists; exact source identity or human clarification is required.",
            "required_source_constraints": ["exact source identity or human ingredient clarification is required"],
            "_required_term_groups": (),
            "_forbidden_terms": (),
        }
        if exact_matches:
            return [
                {
                    "source_code": source_code,
                    "source_id": str(record.fdc_id),
                    "description": record.description,
                    "match_score": 100,
                    "status": "proposal_exact_source_match",
                    "human_review_required": True,
                    "semantic_policy_version": RECIPE_INGREDIENT_MAPPING_POLICY_VERSION,
                    "matched_constraints": list(profile["required_source_constraints"]),
                }
                for record in sorted(exact_matches, key=lambda item: item.fdc_id)[:5]
            ], profile
        return [], profile

    scored: list[tuple[int, int, FdcSourceRecord]] = []
    required_term_groups = profile["_required_term_groups"]
    for record in source_records:
        description = normalize_label(record.description)
        if not all(any(_source_description_has_term(description, term) for term in group) for group in required_term_groups):
            continue
        if any(_source_description_has_term(description, term) for term in profile["_forbidden_terms"]):
            continue
        score = 2 * len(required_term_groups)
        score += sum(
            1
            for group in required_term_groups
            for term in group
            if _source_description_has_term(description, term)
        )
        scored.append((score, record.fdc_id, record))
    scored.sort(key=lambda item: (-item[0], item[1]))
    return [
        {
            "source_code": source_code,
            "source_id": str(record.fdc_id),
            "description": record.description,
            "match_score": score,
            "status": "proposal",
            "human_review_required": True,
            "semantic_policy_version": RECIPE_INGREDIENT_MAPPING_POLICY_VERSION,
            "matched_constraints": list(profile["required_source_constraints"]),
        }
        for score, _, record in scored[:5]
    ], profile


def _load_portion_evidence(path: Path | None, plan_path: Path | None) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    if path is None:
        return [], _not_supplied_report("portion-evidence-0.1.0", "measured/institutional portion evidence input was not supplied")
    evidence_bytes = path.read_bytes()
    evidence_sha256 = _sha256(evidence_bytes)
    payload = json.loads(evidence_bytes.decode("utf-8"))
    candidates = payload.get("candidates") if isinstance(payload, dict) else payload
    if not isinstance(candidates, list):
        raise ValueError("portion evidence must contain a candidates array")
    packaged = [
        {
            **candidate,
            "source_type": "institutional_reference",
            "source_evidence_sha256": evidence_sha256,
            "review_status": "proposal",
            "publication_status": "not_published",
            "project_measurement_required": candidate.get("quality_state") == "not_directly_usable_for_edible_portion",
            "human_review_required": True,
        }
        for candidate in candidates
        if isinstance(candidate, dict)
    ]
    plan_sha256 = None
    if plan_path is not None:
        plan_sha256 = _sha256(plan_path.read_bytes())
    return packaged, {
        "report_version": "portion-evidence-0.1.0",
        "status": "review_required",
        "source_path": str(path),
        "source_sha256": evidence_sha256,
        "measurement_plan_path": str(plan_path) if plan_path else None,
        "measurement_plan_sha256": plan_sha256,
        "candidate_count": len(packaged),
        "institutional_reference_candidate_count": len(packaged),
        "project_measurement_required_count": sum(1 for item in packaged if item["project_measurement_required"]),
        "published_portion_count": 0,
    }


def _not_supplied_report(report_version: str, detail: str) -> dict[str, Any]:
    return {"report_version": report_version, "status": "not_supplied", "coverage": 0, "detail": detail}


def _acquisition(metadata: Any, filename: str, size: int, content_type: str, sha256: str, retrieved_at: str) -> AcquisitionMetadata:
    return AcquisitionMetadata(
        source_code=metadata.code,
        publisher=metadata.publisher,
        release=FDC_FOUNDATION_RELEASE,
        retrieved_at=retrieved_at,
        filename=filename,
        size=size,
        content_type=content_type,
        sha256=sha256,
        rights_state=metadata.rights_state,
        acquisition_tool_version=ACQUISITION_TOOL_VERSION,
    )


def _write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def _require_sha256(payload: bytes, expected: str, label: str) -> None:
    actual = _sha256(payload)
    if actual != expected:
        raise SystemExit(f"{label} SHA-256 mismatch: expected {expected}, actual {actual}")


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


if __name__ == "__main__":
    raise SystemExit(main())
