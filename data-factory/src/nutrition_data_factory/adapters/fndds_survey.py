from __future__ import annotations

import hashlib
import io
import json
import math
import zipfile
from dataclasses import dataclass
from typing import Any

from ..provenance import canonical_json_bytes, json_schema_fingerprint


FNDDS_RELEASE = "2021-2023"
FNDDS_ADAPTER_VERSION = "fndds-survey-json-0.1.0"
FNDDS_ARCHIVE_FILENAME = "FoodData_Central_survey_food_json_2024-10-31.zip"
FNDDS_ARCHIVE_MEMBER = "surveyDownload.json"
FNDDS_EXPECTED_RECORD_COUNT = 5432


@dataclass(frozen=True)
class FnddsReject:
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
class FnddsSourceRecord:
    food_code: str
    fdc_id: int
    description: str
    data_type: str
    wweia_category_description: str
    nutrients: tuple[dict[str, Any], ...]
    payload: dict[str, Any]
    payload_sha256: str


@dataclass(frozen=True)
class FnddsParseResult:
    adapter_version: str
    release: str
    source_sha256: str
    schema_fingerprint: str
    raw_record_count: int
    accepted_records: tuple[FnddsSourceRecord, ...]
    rejected_records: tuple[FnddsReject, ...]

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
            "rejected_records": [item.to_dict() for item in self.rejected_records],
        }


def extract_pinned_archive_member(
    archive_bytes: bytes,
    archive_filename: str,
    source_policy: dict[str, Any],
) -> bytes:
    """Verify the pinned USDA archive before exposing its sole JSON member."""

    if archive_filename != source_policy.get("archive_filename"):
        raise ValueError("archive filename does not match the pinned FNDDS release")
    expected_archive_sha256 = source_policy.get("archive_sha256")
    if _sha256(archive_bytes) != expected_archive_sha256:
        raise ValueError("archive SHA-256 does not match the pinned FNDDS release")

    try:
        with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
            files = [item for item in archive.infolist() if not item.is_dir()]
            if [item.filename for item in files] != [source_policy.get("archive_member")]:
                raise ValueError("archive members do not match the pinned FNDDS release")
            payload = archive.read(files[0])
    except (OSError, zipfile.BadZipFile, RuntimeError) as error:
        raise ValueError("pinned FNDDS archive is not a readable ZIP file") from error

    if _sha256(payload) != source_policy.get("member_sha256"):
        raise ValueError("archive member SHA-256 does not match the pinned FNDDS release")
    return payload


class FnddsSurveyAdapter:
    def parse(
        self,
        payload: bytes,
        *,
        release: str = FNDDS_RELEASE,
        expected_sha256: str | None = None,
    ) -> FnddsParseResult:
        if release != FNDDS_RELEASE:
            raise ValueError(f"unsupported FNDDS release: {release}")
        source_sha256 = _sha256(payload)
        if expected_sha256 is not None and source_sha256 != expected_sha256:
            raise ValueError("FNDDS JSON SHA-256 does not match the expected release")

        try:
            document = json.loads(payload)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ValueError("FNDDS payload is not valid UTF-8 JSON") from error
        if not isinstance(document, dict) or set(document) != {"SurveyFoods"}:
            raise ValueError("FNDDS JSON must contain only the SurveyFoods dataset")
        rows = document["SurveyFoods"]
        if not isinstance(rows, list):
            raise ValueError("FNDDS SurveyFoods must be an array")

        accepted: list[FnddsSourceRecord] = []
        rejected: list[FnddsReject] = []
        seen_food_codes: set[str] = set()
        seen_fdc_ids: set[int] = set()
        for row_index, row in enumerate(rows):
            parsed, reason_code, detail = _parse_record(row)
            if parsed is None:
                rejected.append(FnddsReject(row_index, reason_code, detail))
                continue
            if parsed.food_code in seen_food_codes:
                rejected.append(
                    FnddsReject(row_index, "duplicate_food_code", "FNDDS food code is not unique")
                )
                continue
            if parsed.fdc_id in seen_fdc_ids:
                rejected.append(FnddsReject(row_index, "duplicate_fdc_id", "FDC ID is not unique"))
                continue
            seen_food_codes.add(parsed.food_code)
            seen_fdc_ids.add(parsed.fdc_id)
            accepted.append(parsed)

        return FnddsParseResult(
            adapter_version=FNDDS_ADAPTER_VERSION,
            release=release,
            source_sha256=source_sha256,
            schema_fingerprint=json_schema_fingerprint(document),
            raw_record_count=len(rows),
            accepted_records=tuple(accepted),
            rejected_records=tuple(rejected),
        )


def _parse_record(row: Any) -> tuple[FnddsSourceRecord | None, str, str]:
    if not isinstance(row, dict):
        return None, "invalid_record", "FNDDS food record must be an object"

    food_code = row.get("foodCode")
    if not isinstance(food_code, str) or len(food_code) != 8 or not food_code.isdigit():
        return None, "invalid_food_code", "FNDDS food code must contain eight digits"

    fdc_id = row.get("fdcId")
    if isinstance(fdc_id, bool) or not isinstance(fdc_id, int) or fdc_id <= 0:
        return None, "invalid_fdc_id", "FDC ID must be a positive integer"

    description = row.get("description")
    if not isinstance(description, str) or not description.strip():
        return None, "invalid_description", "FNDDS description must be a non-empty string"
    if row.get("dataType") != "Survey (FNDDS)" or row.get("foodClass") != "Survey":
        return None, "invalid_source_type", "record is not identified as a Survey (FNDDS) food"

    category = row.get("wweiaFoodCategory")
    if not isinstance(category, dict):
        return None, "invalid_category", "FNDDS record must include its WWEIA category"
    category_description = category.get("wweiaFoodCategoryDescription")
    if not isinstance(category_description, str) or not category_description.strip():
        return None, "invalid_category", "FNDDS WWEIA category description is missing"

    nutrients = row.get("foodNutrients")
    if not isinstance(nutrients, list):
        return None, "invalid_nutrients", "FNDDS foodNutrients must be an array"
    for nutrient in nutrients:
        reason = _validate_nutrient(nutrient)
        if reason is not None:
            return None, "invalid_nutrient", reason

    if not isinstance(row.get("foodPortions"), list):
        return None, "invalid_portions_shape", "FNDDS foodPortions must be an array"

    return (
        FnddsSourceRecord(
            food_code=food_code,
            fdc_id=fdc_id,
            description=description,
            data_type="Survey (FNDDS)",
            wweia_category_description=category_description,
            nutrients=tuple(nutrients),
            payload=row,
            payload_sha256=_sha256(canonical_json_bytes(row)),
        ),
        "",
        "",
    )


def _validate_nutrient(nutrient: Any) -> str | None:
    if not isinstance(nutrient, dict):
        return "FNDDS nutrient entry must be an object"
    definition = nutrient.get("nutrient")
    if not isinstance(definition, dict):
        return "FNDDS nutrient definition must be an object"
    source_id = definition.get("id")
    if isinstance(source_id, bool) or not isinstance(source_id, int) or source_id <= 0:
        return "FNDDS source nutrient ID must be a positive integer"
    source_code = definition.get("number")
    if not isinstance(source_code, str) or not source_code.isdigit():
        return "FNDDS nutrient code must be a numeric string"
    if not isinstance(definition.get("name"), str) or not isinstance(definition.get("unitName"), str):
        return "FNDDS nutrient name and unit must be strings"
    amount = nutrient.get("amount")
    if amount is not None:
        if isinstance(amount, bool) or not isinstance(amount, (int, float)):
            return "FNDDS nutrient amount must be a finite number or null"
        if isinstance(amount, float) and not math.isfinite(amount):
            return "FNDDS nutrient amount must be a finite number or null"
    return None


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()
