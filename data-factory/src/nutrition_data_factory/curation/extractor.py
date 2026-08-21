from __future__ import annotations

import hashlib
import json
import re
import unicodedata
from dataclasses import dataclass
from typing import Any


EXTRACTION_POLICY_VERSION = "vietnamese-candidate-extraction-0.1.0"
_QUANTITY_UNIT_RE = re.compile(r"(?P<quantity>\d+(?:[.,]\d+)?)\s*(?P<unit>kg|g|ml|l|bát|chen|chén|quả|miếng|thìa|muỗng)", re.IGNORECASE)
_PREPARATION_TOKENS = ("luộc", "chiên", "nướng", "hấp", "xào", "rán", "sống", "chín", "kho")


@dataclass(frozen=True)
class ExtractionPolicy:
    allow_context_text: bool = False
    private_input: bool = False
    policy_version: str = EXTRACTION_POLICY_VERSION


@dataclass(frozen=True)
class CandidateExtractionResult:
    policy_version: str
    candidates: tuple[dict[str, Any], ...]
    report: dict[str, Any]


def extract_candidates(payload: dict[str, Any], policy: ExtractionPolicy | None = None) -> CandidateExtractionResult:
    policy = policy or ExtractionPolicy()
    observations = payload.get("observations")
    if not isinstance(observations, list):
        raise ValueError("candidate input must contain an observations array")
    grouped: dict[str, dict[str, Any]] = {}
    private_omitted = 0
    for observation_index, observation in enumerate(observations):
        if not isinstance(observation, dict):
            raise ValueError(f"observation {observation_index} must be an object")
        phrase = observation.get("phrase")
        case_id = observation.get("case_id")
        if not isinstance(phrase, str) or not phrase.strip():
            raise ValueError(f"observation {observation_index} phrase must be non-empty")
        if not isinstance(case_id, str) or not case_id.strip():
            raise ValueError(f"observation {observation_index} case_id must be non-empty")
        normalized = normalize_phrase(phrase)
        entry = grouped.setdefault(
            normalized,
            {
                "candidate_id": "candidate-" + hashlib.sha256(normalized.encode("utf-8")).hexdigest()[:20],
                "phrase": phrase.strip(),
                "normalized_phrase": normalized,
                "no_diacritic_search": remove_diacritics(normalized),
                "frequency": 0,
                "distinct_case_ids": set(),
                "context_refs": [],
                "quantity_units": set(),
                "preparation_tokens": set(),
                "source_classes": set(),
                "negated_observation_count": 0,
                "machine_confidence": {"value": None, "authoritative": False},
                "review_status": "proposal",
            },
        )
        entry["frequency"] += 1
        entry["distinct_case_ids"].add(case_id)
        source_class = observation.get("source_class")
        if isinstance(source_class, str) and source_class.strip():
            entry["source_classes"].add(source_class)
        if observation.get("negated") is True:
            entry["negated_observation_count"] += 1
        source_text = " ".join(str(value) for value in (phrase, observation.get("context", "")))
        entry["quantity_units"].update(
            f"{match.group('quantity')} {match.group('unit').lower()}"
            for match in _QUANTITY_UNIT_RE.finditer(source_text)
        )
        entry["preparation_tokens"].update(
            token for token in _PREPARATION_TOKENS if token in source_text.casefold()
        )
        context = observation.get("context")
        context_allowed = observation.get("context_allowed", False)
        context_ref = {
            "case_id": case_id,
            "observation_index": observation_index,
        }
        if policy.allow_context_text and not policy.private_input and context_allowed and isinstance(context, str):
            context_ref["safe_context"] = context
        else:
            if context is not None:
                private_omitted += 1
            context_ref["context_sha256"] = hashlib.sha256(str(context or "").encode("utf-8")).hexdigest()
        entry["context_refs"].append(context_ref)
    candidates = []
    for entry in grouped.values():
        candidate = dict(entry)
        candidate["distinct_case_ids"] = sorted(entry["distinct_case_ids"])
        candidate["context_refs"] = sorted(entry["context_refs"], key=lambda item: (item["case_id"], item["observation_index"]))
        candidate["quantity_units"] = sorted(entry["quantity_units"])
        candidate["preparation_tokens"] = sorted(entry["preparation_tokens"])
        candidate["source_classes"] = sorted(entry["source_classes"])
        candidate["negated_observation_count"] = entry["negated_observation_count"]
        candidate["rank_key"] = [
            -entry["frequency"],
            -len(entry["distinct_case_ids"]),
            candidate["normalized_phrase"],
        ]
        candidates.append(candidate)
    candidates.sort(key=lambda item: item["rank_key"])
    for rank, candidate in enumerate(candidates, start=1):
        candidate["rank"] = rank
        del candidate["rank_key"]
    return CandidateExtractionResult(
        policy_version=policy.policy_version,
        candidates=tuple(candidates),
        report={
            "policy_version": policy.policy_version,
            "input_observation_count": len(observations),
            "unique_candidate_count": len(candidates),
            "private_context_omitted_count": private_omitted,
            "nutrition_values_emitted": False,
        },
    )


def normalize_phrase(phrase: str) -> str:
    return " ".join(unicodedata.normalize("NFKC", phrase).split())


def remove_diacritics(value: str) -> str:
    decomposed = unicodedata.normalize("NFD", value)
    return "".join(character for character in decomposed if unicodedata.category(character) != "Mn").casefold()
