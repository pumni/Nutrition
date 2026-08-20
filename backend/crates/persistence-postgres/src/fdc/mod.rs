//! FDC importer facade. Responsibility-specific files document the retrieval boundaries while
//! the behavior-preserving implementation remains behind this narrow public surface.

mod artifact;
mod energy;
mod implementation;
mod model;
mod normalize;
mod nutrients;
mod parse;
mod provenance;
mod report;
mod selection;
mod staging;

pub use implementation::{
    FDC_ENERGY_MAPPING_POLICY_VERSION, FDC_FOUNDATION_2026_04_ARCHIVE_SHA256,
    FDC_FOUNDATION_2026_04_EXTRACTED_JSON_SHA256, FDC_FOUNDATION_2026_04_NULL_TAIL_POLICY_VERSION,
    FDC_FOUNDATION_2026_04_RELEASE_VERSION, FDC_FOUNDATION_IMPORTER_VERSION,
    FDC_FOUNDATION_V1_SELECTION_CAP, FDC_FOUNDATION_V1_SELECTION_REVIEWER,
    FdcFoundationImportError, FdcFoundationImportReport, FdcFoundationImportRequest,
    FdcFoundationValidationReport, FdcFoundationValidationRequest,
    build_fdc_selection_candidate_manifest, import_fdc_foundation_json,
    validate_fdc_foundation_json,
};
