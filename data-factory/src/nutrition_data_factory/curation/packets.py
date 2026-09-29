from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from typing import Any


CURATION_DECISION_POLICY_VERSION = "curation-decision-0.2.0"
ALLOWED_DECISIONS = frozenset({"approved", "rejected", "ambiguous", "deferred"})
FORBIDDEN_REVIEWER_MARKERS = ("ai", "agent", "llm", "model", "bot")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")


@dataclass(frozen=True)
class SourceRecordIdentity:
    source_code: str
    release: str
    source_record_id: str
    external_id_type: str
    external_record_id: str
    description: str
    record_sha256: str
    archive_sha256: str
    archive_member: str
    member_sha256: str
    schema_fingerprint: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_code": self.source_code,
            "release": self.release,
            "source_record_id": self.source_record_id,
            "external_id_type": self.external_id_type,
            "external_record_id": self.external_record_id,
            "description": self.description,
            "record_sha256": self.record_sha256,
            "archive_sha256": self.archive_sha256,
            "archive_member": self.archive_member,
            "member_sha256": self.member_sha256,
            "schema_fingerprint": self.schema_fingerprint,
        }


@dataclass(frozen=True)
class CurationDecision:
    decision_id: str
    decision_type: str
    candidate_id: str
    decision: str
    reviewer: str
    reviewed_at: str
    rationale: str
    evidence_refs: tuple[str, ...]
    policy_version: str
    supersedes: str | None = None
    decision_reference: str | None = None
    source_record_identity: SourceRecordIdentity | None = None

    def to_dict(self) -> dict[str, Any]:
        result = {
            "decision_id": self.decision_id,
            "decision_type": self.decision_type,
            "candidate_id": self.candidate_id,
            "decision": self.decision,
            "reviewer": self.reviewer,
            "reviewed_at": self.reviewed_at,
            "rationale": self.rationale,
            "evidence_refs": list(self.evidence_refs),
            "policy_version": self.policy_version,
            "supersedes": self.supersedes,
        }
        if self.policy_version == CURATION_DECISION_POLICY_VERSION:
            result["decision_reference"] = self.decision_reference
            result["source_record_identity"] = (
                self.source_record_identity.to_dict() if self.source_record_identity else None
            )
        return result


def build_review_packet(
    candidate: dict[str, Any],
    *,
    candidate_concept: dict[str, Any] | None = None,
    source_foods: list[dict[str, Any]] | None = None,
    evidence_refs: list[str] | None = None,
) -> dict[str, Any]:
    return {
        "packet_version": "curation-review-packet-0.1.0",
        "candidate_id": candidate.get("candidate_id"),
        "observed_phrase": candidate.get("phrase"),
        "normalized_phrase": candidate.get("normalized_phrase"),
        "candidate_concept": candidate_concept,
        "source_foods": source_foods or [],
        "preparation_differences": candidate.get("preparation_tokens", []),
        "scientific_name_differences": [],
        "geographic_differences": [],
        "evidence_refs": sorted(evidence_refs or []),
        "machine_confidence": {
            "value": candidate.get("machine_confidence", {}).get("value"),
            "authoritative": False,
        },
        "review_status": "pending_human_review",
        "decision": None,
    }


class DecisionHistory:
    def __init__(self) -> None:
        self._decisions: dict[str, CurationDecision] = {}

    def append(self, decision: CurationDecision) -> None:
        validate_decision(decision)
        if decision.decision_id in self._decisions:
            raise ValueError(f"duplicate decision_id: {decision.decision_id}")
        if decision.supersedes is not None and decision.supersedes not in self._decisions:
            raise ValueError(f"superseded decision not found: {decision.supersedes}")
        self._decisions[decision.decision_id] = decision

    def all(self) -> tuple[CurationDecision, ...]:
        return tuple(self._decisions[key] for key in sorted(self._decisions))

    def to_jsonl(self) -> str:
        return "".join(
            json.dumps(decision.to_dict(), ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n"
            for decision in self.all()
        )


def validate_decision(decision: CurationDecision) -> None:
    if decision.decision not in ALLOWED_DECISIONS:
        raise ValueError(f"unsupported curation decision: {decision.decision}")
    if not decision.reviewer.strip():
        raise ValueError("reviewer is required")
    lowered = decision.reviewer.casefold()
    if any(marker in lowered for marker in FORBIDDEN_REVIEWER_MARKERS):
        raise ValueError("machine identities cannot act as human reviewer")
    if not decision.evidence_refs:
        raise ValueError("at least one evidence reference is required")
    if (
        decision.policy_version != CURATION_DECISION_POLICY_VERSION
        and (decision.decision_reference is not None or decision.source_record_identity is not None)
    ):
        raise ValueError("structured curation evidence requires curation-decision-0.2.0")
    if decision.source_record_identity is not None:
        _validate_source_record_identity(decision.source_record_identity)
    if decision.decision_type == "food_source_mapping":
        if decision.policy_version != CURATION_DECISION_POLICY_VERSION:
            raise ValueError("food source mapping decisions require curation-decision-0.2.0")
        if not decision.decision_reference or not decision.decision_reference.strip():
            raise ValueError("food source mapping decisions require a human decision reference")
        if decision.source_record_identity is None:
            raise ValueError("food source mapping decisions require structured source record identity")


def _validate_source_record_identity(identity: SourceRecordIdentity) -> None:
    for value in (
        identity.source_code,
        identity.release,
        identity.source_record_id,
        identity.external_id_type,
        identity.external_record_id,
        identity.description,
        identity.archive_member,
    ):
        if not value.strip():
            raise ValueError("source record identity fields must be non-empty")
    for value in (
        identity.record_sha256,
        identity.archive_sha256,
        identity.member_sha256,
        identity.schema_fingerprint,
    ):
        if not SHA256_PATTERN.fullmatch(value):
            raise ValueError("source record identity hashes must be lowercase SHA-256 values")


def decision_id(candidate_id: str, reviewed_at: str, reviewer: str) -> str:
    raw = f"{candidate_id}:{reviewed_at}:{reviewer}".encode("utf-8")
    return "CUR-" + hashlib.sha256(raw).hexdigest()[:20]
