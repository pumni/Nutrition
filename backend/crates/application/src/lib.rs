mod analyze;
mod model;
mod ports;

pub use analyze::{AnalyzeMeal, DirectAnalysisService};
pub use model::{
    AnalysisItemSnapshot, AnalysisMode, AnalysisRequest, AnalysisSnapshot, AnalysisStatus,
    BehaviorVersions, ParseRequest, ParsedMealDocument, ParsedMealItem, ResolvedEvidence,
};
pub use ports::{AnalysisRepository, ApplicationError, CatalogEvidenceProvider, MealTextParser};
