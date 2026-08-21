from __future__ import annotations

import json
from typing import Any

from ..models import SourceMetadata, SourceRecord


class SyntheticAdapter:
    """Adapter for the non-nutrition synthetic fixture used by DATA-000."""

    schema_version = "synthetic-source-0.1.0"

    def parse(self, payload: bytes, metadata: SourceMetadata) -> list[SourceRecord]:
        root = json.loads(payload)
        if not isinstance(root, dict) or root.get("schema_version") != self.schema_version:
            raise ValueError(f"expected {self.schema_version}")
        if root.get("source_code") != metadata.code:
            raise ValueError("fixture source_code does not match the source registry")
        if root.get("release") != metadata.release:
            raise ValueError("fixture release does not match the source registry")
        raw_records = root.get("records")
        if not isinstance(raw_records, list) or not raw_records:
            raise ValueError("fixture records must be a non-empty array")
        records: list[SourceRecord] = []
        seen: set[str] = set()
        for raw_record in raw_records:
            if not isinstance(raw_record, dict):
                raise ValueError("fixture records must be objects")
            source_id = raw_record.get("source_id")
            label = raw_record.get("label")
            attributes = raw_record.get("attributes")
            if not isinstance(source_id, str) or not source_id.strip():
                raise ValueError("fixture source_id must be a non-empty string")
            if source_id in seen:
                raise ValueError(f"duplicate fixture source_id: {source_id}")
            if not isinstance(label, str) or not label.strip():
                raise ValueError(f"fixture label missing for {source_id}")
            if not isinstance(attributes, dict):
                raise ValueError(f"fixture attributes missing for {source_id}")
            seen.add(source_id)
            records.append(
                SourceRecord(
                    source_code=metadata.code,
                    release=metadata.release,
                    source_id=source_id,
                    label=label,
                    attributes=attributes,
                )
            )
        return sorted(records, key=lambda record: record.source_id)

