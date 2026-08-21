from __future__ import annotations

import unicodedata

from ..models import NormalizedRecord, SourceRecord


NORMALIZATION_POLICY_VERSION = "synthetic-normalization-0.1.0"


def normalize_records(records: list[SourceRecord]) -> list[NormalizedRecord]:
    normalized = [
        NormalizedRecord(
            source_code=record.source_code,
            release=record.release,
            source_id=record.source_id,
            original_label=record.label,
            normalized_label=normalize_label(record.label),
            attributes=record.attributes,
        )
        for record in records
    ]
    return sorted(normalized, key=lambda record: record.source_id)


def normalize_label(label: str) -> str:
    folded = unicodedata.normalize("NFKC", label)
    return " ".join(folded.split()).casefold()

