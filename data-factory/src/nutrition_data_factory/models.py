from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class SourceMetadata:
    code: str
    publisher: str
    purpose: str
    release: str
    locator: str
    status_initial: str
    rights_state: str
    production_ingestion: str
    access_status: str
    approval_reference: str
    allowed_uses: tuple[str, ...]
    prohibited_uses: tuple[str, ...]
    priority: int
    production_eligible: bool

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> "SourceMetadata":
        return cls(
            code=_required_string(value, "code"),
            publisher=_required_string(value, "publisher"),
            purpose=_required_string(value, "purpose"),
            release=_required_string(value, "release"),
            locator=_required_string(value, "locator"),
            status_initial=_required_string(value, "status_initial"),
            rights_state=_required_string(value, "rights_state"),
            production_ingestion=_required_string(value, "production_ingestion"),
            access_status=_required_string(value, "access_status"),
            approval_reference=_required_string(value, "approval_reference"),
            allowed_uses=tuple(_string_list(value, "allowed_uses")),
            prohibited_uses=tuple(_string_list(value, "prohibited_uses")),
            priority=_required_int(value, "priority"),
            production_eligible=_required_bool(value, "production_eligible"),
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "code": self.code,
            "publisher": self.publisher,
            "purpose": self.purpose,
            "release": self.release,
            "locator": self.locator,
            "status_initial": self.status_initial,
            "rights_state": self.rights_state,
            "production_ingestion": self.production_ingestion,
            "access_status": self.access_status,
            "approval_reference": self.approval_reference,
            "allowed_uses": list(self.allowed_uses),
            "prohibited_uses": list(self.prohibited_uses),
            "priority": self.priority,
            "production_eligible": self.production_eligible,
        }


@dataclass(frozen=True)
class SourceRecord:
    source_code: str
    release: str
    source_id: str
    label: str
    attributes: dict[str, Any]

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_code": self.source_code,
            "release": self.release,
            "source_id": self.source_id,
            "label": self.label,
            "attributes": self.attributes,
        }


@dataclass(frozen=True)
class NormalizedRecord:
    source_code: str
    release: str
    source_id: str
    original_label: str
    normalized_label: str
    attributes: dict[str, Any]

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_code": self.source_code,
            "release": self.release,
            "source_id": self.source_id,
            "original_label": self.original_label,
            "normalized_label": self.normalized_label,
            "attributes": self.attributes,
        }


@dataclass(frozen=True)
class ValidationReport:
    rule_version: str
    passed: bool
    errors: tuple[str, ...]
    warnings: tuple[str, ...]
    statistics: dict[str, int]

    def to_dict(self) -> dict[str, Any]:
        return {
            "rule_version": self.rule_version,
            "passed": self.passed,
            "errors": list(self.errors),
            "warnings": list(self.warnings),
            "statistics": self.statistics,
        }


def _required_string(value: dict[str, Any], key: str) -> str:
    item = value.get(key)
    if not isinstance(item, str) or not item.strip():
        raise ValueError(f"{key} must be a non-empty string")
    return item


def _required_int(value: dict[str, Any], key: str) -> int:
    item = value.get(key)
    if isinstance(item, bool) or not isinstance(item, int):
        raise ValueError(f"{key} must be an integer")
    return item


def _required_bool(value: dict[str, Any], key: str) -> bool:
    item = value.get(key)
    if not isinstance(item, bool):
        raise ValueError(f"{key} must be a boolean")
    return item


def _string_list(value: dict[str, Any], key: str) -> list[str]:
    item = value.get(key)
    if not isinstance(item, list) or not all(isinstance(entry, str) for entry in item):
        raise ValueError(f"{key} must be a list of strings")
    return item
