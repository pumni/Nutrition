from __future__ import annotations

from ..artifacts import ArtifactRef
from ..models import NormalizedRecord, SourceMetadata, ValidationReport
from ..source_registry import production_rights_error


VALIDATION_RULE_VERSION = "scaffold-validation-0.2.0"


def validate_records(
    records: list[NormalizedRecord],
    metadata: SourceMetadata,
    artifact: ArtifactRef,
    production_candidate: bool = False,
) -> ValidationReport:
    errors: list[str] = []
    warnings: list[str] = []
    source_ids = [record.source_id for record in records]
    if not records:
        errors.append("no normalized records")
    if len(source_ids) != len(set(source_ids)):
        errors.append("duplicate normalized source_id")
    if any(record.source_code != metadata.code for record in records):
        errors.append("record source_code does not match registry")
    if any(record.release != metadata.release for record in records):
        errors.append("record release does not match registry")
    if metadata.production_eligible:
        errors.append(f"source {metadata.code} cannot be production eligible in this scaffold")
    if production_candidate:
        rights_error = production_rights_error(metadata)
        if rights_error is not None:
            errors.append(rights_error)
    if len(artifact.sha256) != 64 or any(character not in "0123456789abcdef" for character in artifact.sha256):
        errors.append("artifact reference is not a SHA-256 digest")
    if not metadata.approval_reference:
        errors.append("source approval reference is missing")
    if metadata.rights_state == "test_only":
        warnings.append("synthetic fixture is test-only evidence and is not a nutrition source")
    return ValidationReport(
        rule_version=VALIDATION_RULE_VERSION,
        passed=not errors,
        errors=tuple(sorted(errors)),
        warnings=tuple(sorted(warnings)),
        statistics={
            "normalized_record_count": len(records),
            "unique_source_id_count": len(set(source_ids)),
        },
    )
