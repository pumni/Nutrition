#![allow(clippy::wildcard_imports)]

use super::*;

impl FdcFoundationValidationReport {
    #[must_use]
    pub fn to_json(&self, request: &FdcFoundationValidationRequest) -> Value {
        json!({
            "source": FDC_DATASET_CODE,
            "source_release": request.release_version,
            "source_published_date": request.source_published_date,
            "object_uri": request.object_uri,
            "source_payload_filename": request.source_payload_filename,
            "source_archive_sha256": request.source_archive_sha256,
            "artifact_sha256": self.source_sha256,
            "expected_artifact_sha256": self.expected_sha256,
            "checksum_valid": self.checksum_status == "valid",
            "source_integrity_valid": self.source_integrity_valid.is_valid(),
            "source_schema_conformant": self.source_schema_conformant.is_valid(),
            "preprocessing_applied": self.preprocessing_applied.is_applied(),
            "preprocessing_policy_version": self.preprocessing_policy_version,
            "normalized_payload_sha256": self.normalized_payload_sha256,
            "normalized_record_count": self.normalized_record_count,
            "normalized_payload_valid": self.normalized_payload_valid.is_valid(),
            "schema_fingerprint": self.schema_fingerprint,
            "schema_contract": FDC_SCHEMA_CONTRACT,
            "importer_version": FDC_FOUNDATION_IMPORTER_VERSION,
            "energy_policy": FDC_ENERGY_MAPPING_POLICY_VERSION,
            "raw_records": self.raw_record_count,
            "valid_records": self.valid_record_count,
            "selected_records": self.selected_record_count,
            "null_records": self.null_record_count,
            "invalid_records": self.invalid_record_count,
            "source_schema_errors": self.source_schema_errors,
            "selection_fingerprint": self.selection_fingerprint,
            "selection_status": self.selection_status,
            "energy": {
                "scope": "valid_source_records",
                "atwater_specific_2048": self.source_energy_atwater_specific_count,
                "atwater_general_2047": self.source_energy_atwater_general_count,
                "missing": self.source_energy_missing_count,
                "unexpected_legacy_1008": self.source_unexpected_legacy_energy_count
            },
            "selected_energy": {
                "scope": "reviewed_selection",
                "atwater_specific_2048": self.selected_energy_atwater_specific_count,
                "atwater_general_2047": self.selected_energy_atwater_general_count,
                "missing": self.selected_energy_missing_count,
                "unexpected_legacy_1008": self.selected_unexpected_legacy_energy_count
            },
            "errors": self.errors,
            "artifact_valid": self.artifact_status == "valid",
            "selection_valid": self.selection_status == "validated",
            "validation_passed": self.validation_status == "passed",
            "production_eligible": false,
            "release": {
                "status": "staged_only",
                "reviewer_approved": false,
                "activation_attempted": false
            }
        })
    }

    /// Renders the report as deterministic pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if the report cannot be represented as JSON.
    pub fn to_pretty_json(
        &self,
        request: &FdcFoundationValidationRequest,
    ) -> Result<String, serde_json::Error> {
        // `Value` contains only JSON-native values, so this serialization is deterministic.
        serde_json::to_string_pretty(&self.to_json(request))
    }
}

pub(crate) fn import_report(
    prepared: &PreparedImport,
    dataset_release_id: Uuid,
    catalog_release_id: Uuid,
    replayed: bool,
) -> FdcFoundationImportReport {
    FdcFoundationImportReport {
        dataset_release_id,
        catalog_release_id,
        catalog_release_version: prepared.catalog_release_version.clone(),
        raw_record_count: prepared.foods.len(),
        selected_record_count: prepared.selected_ids.len(),
        source_sha256: prepared.source_sha256.clone(),
        schema_fingerprint: prepared.schema_fingerprint.clone(),
        energy_atwater_specific_count: prepared.energy_summary.atwater_specific,
        energy_atwater_general_count: prepared.energy_summary.atwater_general,
        energy_missing_count: prepared.energy_summary.missing_energy,
        unexpected_legacy_energy_count: prepared.energy_summary.unexpected_legacy,
        replayed,
    }
}
