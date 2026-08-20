from __future__ import annotations

import hashlib
import json
from typing import Any

from .models import SourceRecord


PROVENANCE_POLICY_VERSION = "provenance-core-0.1.0"


def canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def source_record_hash(record: SourceRecord) -> str:
    """Hash the canonical source-record serialization, independent of dict ordering."""

    return hashlib.sha256(canonical_json_bytes(record.to_dict())).hexdigest()


def json_schema_fingerprint(value: Any) -> str:
    """Fingerprint JSON structure without including source values."""

    signature = _shape(value)
    return hashlib.sha256(canonical_json_bytes(signature)).hexdigest()


def _shape(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _shape(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        shapes = [_shape(item) for item in value]
        unique = {json.dumps(item, ensure_ascii=False, sort_keys=True, separators=(",", ":")) for item in shapes}
        return {"array_items": [json.loads(item) for item in sorted(unique)]}
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int) and not isinstance(value, bool):
        return "integer"
    if isinstance(value, float):
        return "number"
    if isinstance(value, str):
        return "string"
    raise TypeError(f"unsupported JSON value type: {type(value).__name__}")

