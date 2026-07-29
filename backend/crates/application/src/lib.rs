mod analyze;
mod model;
mod normalization;
mod ports;
mod revise;

pub use analyze::{AnalyzeMeal, MealAnalysisService};
pub use model::{
    AnalysisItemSnapshot, AnalysisMode, AnalysisOutcome, AnalysisRequest, AnalysisSnapshot,
    AnalysisStatus, BehaviorVersions, ClarificationAnalysis, ClarificationAnswerRequest,
    ClarificationContext, ClarificationOption, ClarificationQuestion, CorrectionRequest,
    IdempotencyContext, ParseRequest, ParsedMealDocument, ParsedMealItem, ParserInvocationRecord,
    PortionCorrection, PortionSuggestion, ResolvedFoodEvidence, ResolvedPortionEvidence,
};
pub use normalization::normalize_vi_search_key;
pub use ports::{
    AnalysisRepository, AnalysisSnapshotReader, ApplicationError, FoodEvidenceProvider,
    MealTextParser, ParserTelemetrySink, PortionEvidenceProvider,
};
pub use revise::{AnalysisRevisionService, AnswerClarification, CorrectAnalysis};
