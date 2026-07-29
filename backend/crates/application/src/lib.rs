mod analyze;
mod model;
mod normalization;
mod ports;

pub use analyze::{AnalyzeMeal, DirectAnalysisService};
pub use model::{
    AnalysisItemSnapshot, AnalysisMode, AnalysisRequest, AnalysisSnapshot, AnalysisStatus,
    BehaviorVersions, ParseRequest, ParsedMealDocument, ParsedMealItem, ResolvedEvidence,
};
pub use normalization::normalize_vi_search_key;
pub use ports::{
    AnalysisRepository, AnalysisSnapshotReader, ApplicationError, CatalogEvidenceProvider,
    MealTextParser,
};
