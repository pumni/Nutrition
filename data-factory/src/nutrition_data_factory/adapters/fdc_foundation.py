from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from typing import Any

from ..provenance import json_schema_fingerprint


FDC_FOUNDATION_RELEASE = "2026-04-30"
FDC_ADAPTER_VERSION = "fdc-foundation-json-0.1.0"


@dataclass(frozen=True)
class FdcReject:
    row_index: int
    reason_code: str
    detail: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "row_index": self.row_index,
            "reason_code": self.reason_code,
            "detail": self.detail,
        }


@dataclass(frozen=True)
class FdcSourceRecord:
    fdc_id: int
    description: str
    data_type: str
    characteristics: Any
    nutrients: tuple[dict[str, Any], ...]
    portions: tuple[dict[str, Any], ...]
    payload: dict[str, Any]
    payload_sha256: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "fdc_id": self.fdc_id,
            "description": self.description,
            "data_type": self.data_type,
            "characteristics": self.characteristics,
            "nutrients": list(self.nutrients),
            "portions": list(self.portions),
            "payload": self.payload,
            "payload_sha256": self.payload_sha256,
        }


@dataclass(frozen=True)
class FdcParseResult:
    adapter_version: str
    release: str
    source_sha256: str
    schema_fingerprint: str
    raw_record_count: int
    accepted_records: tuple[FdcSourceRecord, ...]
    rejected_records: tuple[FdcReject, ...]

    @property
    def valid(self) -> bool:
        return not self.rejected_records

    def to_dict(self) -> dict[str, Any]:
        return {
            "adapter_version": self.adapter_version,
            "release": self.release,
            "source_sha256": self.source_sha256,
            "schema_fingerprint": self.schema_fingerprint,
            "raw_record_count": self.raw_record_count,
            "accepted_record_count": len(self.accepted_records),
            "rejected_record_count": len(self.rejected_records),
            "accepted_records": [record.to_dict() for record in self.accepted_records],
            "rejected_records": [reject.to_dict() for reject in self.rejected_records],
        }


class FdcFoundationAdapter:
    """Offline parser for a caller-supplied, release-pinned Foundation Foods JSON artifact."""

    def parse(
        self,
        payload: bytes,
        *,
        release: str = FDC_FOUNDATION_RELEASE,
        expected_sha256: str | None = None,
    ) -> FdcParseResult:
        if release != FDC_FOUNDATION_RELEASE:
            raise ValueError(
                f"unsupported FDC Foundation release {release}; expected {FDC_FOUNDATION_RELEASE}"
            )
        source_sha256 = hashlib.sha256(payload).hexdigest()
        if expected_sha256 is not None and source_sha256 != expected_sha256.lower():
            raise ValueError(
                f"FDC artifact checksum mismatch: expected {expected_sha256.lower()}, actual {source_sha256}"
            )
        try:
            root = json.loads(payload)
        except json.JSONDecodeError as error:
            raise ValueError(f"FDC artifact is not valid JSON: {error.msg}") from error
        if not isinstance(root, dict) or not isinstance(root.get("FoundationFoods"), list):
            raise ValueError("FDC artifact must contain a FoundationFoods array")
        rows = root["FoundationFoods"]
        accepted: list[FdcSourceRecord] = []
        rejected: list[FdcReject] = []
        seen_ids: set[int] = set()
        for row_index, row in enumerate(rows):
            result = self._parse_row(row_index, row, seen_ids)
            if isinstance(result, FdcReject):
                rejected.append(result)
            else:
                accepted.append(result)
        return FdcParseResult(
            adapter_version=FDC_ADAPTER_VERSION,
            release=release,
            source_sha256=source_sha256,
            schema_fingerprint=json_schema_fingerprint(root),
            raw_record_count=len(rows),
            accepted_records=tuple(accepted),
            rejected_records=tuple(rejected),
        )

    @staticmethod
    def _parse_row(
        row_index: int,
        row: Any,
        seen_ids: set[int],
    ) -> FdcSourceRecord | FdcReject:
        if row is None:
            return FdcReject(row_index, "null_record", "FoundationFoods row is null")
        if not isinstance(row, dict):
            return FdcReject(row_index, "record_not_object", "FoundationFoods row is not an object")
        fdc_id = row.get("fdcId")
        if isinstance(fdc_id, bool) or not isinstance(fdc_id, int):
            return FdcReject(row_index, "invalid_fdc_id", "fdcId must be an integer")
        if fdc_id in seen_ids:
            return FdcReject(row_index, "duplicate_fdc_id", f"duplicate fdcId={fdc_id}")
        data_type = row.get("dataType")
        if data_type != "Foundation":
            return FdcReject(row_index, "wrong_data_type", "dataType must be Foundation")
        description = row.get("description")
        if not isinstance(description, str) or not description.strip():
            return FdcReject(row_index, "missing_description", "description must be non-empty")
        nutrients = row.get("foodNutrients")
        if not isinstance(nutrients, list):
            return FdcReject(row_index, "missing_nutrients", "foodNutrients must be an array")
        portions = row.get("foodPortions", [])
        if not isinstance(portions, list) or not all(isinstance(item, dict) for item in portions):
            return FdcReject(row_index, "invalid_portions", "foodPortions must be an array of objects")
        for nutrient_index, nutrient in enumerate(nutrients):
            if not isinstance(nutrient, dict) or not isinstance(nutrient.get("nutrient"), dict):
                return FdcReject(
                    row_index,
                    "invalid_nutrient",
                    f"foodNutrients[{nutrient_index}] must contain a nutrient object",
                )
        seen_ids.add(fdc_id)
        payload_bytes = json.dumps(
            row,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")
        return FdcSourceRecord(
            fdc_id=fdc_id,
            description=description,
            data_type=data_type,
            characteristics=row.get("foodCharacteristics", []),
            nutrients=tuple(nutrients),
            portions=tuple(portions),
            payload=row,
            payload_sha256=hashlib.sha256(payload_bytes).hexdigest(),
        )

