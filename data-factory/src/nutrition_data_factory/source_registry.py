from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from .models import SourceMetadata

REGISTRY_SCHEMA_VERSION = "source-registry-0.1.0"
RIGHTS_STATES = frozenset(
    {"approved", "reference_only", "pending_unknown", "prohibited", "test_only"}
)


class SourceRegistryError(ValueError):
    pass


class SourceRegistry:
    def __init__(self, sources: dict[str, SourceMetadata]) -> None:
        self._sources = sources

    @classmethod
    def load(cls, path: Path) -> "SourceRegistry":
        with path.open("r", encoding="utf-8") as handle:
            raw = json.load(handle)
        if not isinstance(raw, dict) or raw.get("schema_version") != REGISTRY_SCHEMA_VERSION:
            raise SourceRegistryError(f"source registry must use {REGISTRY_SCHEMA_VERSION}")
        if not isinstance(raw.get("sources"), list):
            raise SourceRegistryError("source registry must contain a sources array")
        sources: dict[str, SourceMetadata] = {}
        for entry in raw["sources"]:
            if not isinstance(entry, dict):
                raise SourceRegistryError("source registry entries must be objects")
            metadata = SourceMetadata.from_dict(entry)
            _validate_metadata(metadata)
            if metadata.code in sources:
                raise SourceRegistryError(f"duplicate source code: {metadata.code}")
            sources[metadata.code] = metadata
        return cls(sources)

    def get(self, code: str) -> SourceMetadata:
        try:
            return self._sources[code]
        except KeyError as error:
            raise SourceRegistryError(f"unknown source code: {code}") from error

    def assert_test_use_allowed(self, metadata: SourceMetadata) -> None:
        if "unit_test" not in metadata.allowed_uses and "scaffold_demo" not in metadata.allowed_uses:
            raise SourceRegistryError(
                f"source {metadata.code} is not approved for scaffold test use"
            )
        if metadata.production_eligible:
            raise SourceRegistryError(
                f"synthetic scaffold source {metadata.code} cannot be production eligible"
            )

    def assert_can_contribute_to_production(self, metadata: SourceMetadata) -> None:
        error = production_rights_error(metadata)
        if error is not None:
            raise SourceRegistryError(error)

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": REGISTRY_SCHEMA_VERSION,
            "sources": [self._sources[key].to_dict() for key in sorted(self._sources)],
        }


def production_rights_error(metadata: SourceMetadata) -> str | None:
    """Return a fail-closed error when rights do not authorize a staged candidate."""

    if metadata.rights_state != "approved":
        return (
            f"source {metadata.code} cannot contribute to a production candidate: "
            f"rights_state={metadata.rights_state}"
        )
    if "staged_candidate" not in metadata.allowed_uses:
        return f"source {metadata.code} lacks staged_candidate in allowed_uses"
    if "staged_candidate" in metadata.prohibited_uses or "production" in metadata.prohibited_uses:
        return f"source {metadata.code} prohibits staged candidate contribution"
    return None


def _validate_metadata(metadata: SourceMetadata) -> None:
    if metadata.rights_state not in RIGHTS_STATES:
        raise SourceRegistryError(
            f"source {metadata.code} has unsupported rights_state={metadata.rights_state}"
        )
    if metadata.priority < 0:
        raise SourceRegistryError(f"source {metadata.code} priority cannot be negative")
    if metadata.production_eligible and metadata.rights_state != "approved":
        raise SourceRegistryError(
            f"source {metadata.code} cannot be production_eligible with "
            f"rights_state={metadata.rights_state}"
        )
    if metadata.rights_state == "prohibited" and metadata.production_ingestion != "prohibited":
        raise SourceRegistryError(
            f"prohibited source {metadata.code} must have production_ingestion=prohibited"
        )
