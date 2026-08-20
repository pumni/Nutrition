use crate::{
    AnalysisListEntry, AnalysisListQuery, AnalysisSnapshot, AnalysisWorkflow,
    ClarificationAnalysis, ClarificationAnswerRequest, CorrectionRequest, ParseRequest,
    ParsedMealDocument, ParsedMealItem, ParserInvocationRecord, PortionSuggestion,
    ResolvedFoodEvidence, ResolvedPortionEvidence,
};
use async_trait::async_trait;
use domain::{AnalysisId, AnalysisRevisionId, FoodId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("invalid cursor")]
    InvalidCursor,
    #[error("meal parser unavailable: {0}")]
    ParserUnavailable(String),
    #[error("food or portion evidence is insufficient: {0}")]
    InsufficientEvidence(String),
    #[error("calculation failed: {0}")]
    Calculation(String),
    #[error("analysis persistence failed")]
    Persistence,
    #[error("analysis was not found")]
    NotFound,
    #[error("analysis revision is stale")]
    RevisionConflict,
    #[error("clarification question or expected revision is stale")]
    StaleClarification,
    #[error("idempotency key was reused with a different request")]
    IdempotencyConflict,
    #[error("authentication is required")]
    Unauthorized,
    #[error("the authenticated principal cannot access this analysis")]
    Forbidden,
}

#[async_trait]
pub trait MealTextParser: Send + Sync {
    async fn parse(&self, request: ParseRequest) -> Result<ParsedMealDocument, ApplicationError>;
}

#[async_trait]
pub trait ParserTelemetrySink: Send + Sync {
    async fn record(&self, invocation: ParserInvocationRecord) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait FoodEvidenceProvider: Send + Sync {
    async fn resolve_food(
        &self,
        locale: &str,
        item: &ParsedMealItem,
    ) -> Result<ResolvedFoodEvidence, ApplicationError>;
}

#[async_trait]
pub trait PortionEvidenceProvider: Send + Sync {
    async fn resolve_portion(
        &self,
        locale: &str,
        item: &ParsedMealItem,
        food_id: FoodId,
    ) -> Result<ResolvedPortionEvidence, ApplicationError>;

    async fn suggestions(
        &self,
        locale: &str,
        food_id: FoodId,
    ) -> Result<Vec<PortionSuggestion>, ApplicationError>;
}

#[async_trait]
pub trait AnalysisRepository: Send + Sync {
    async fn save(&self, snapshot: &AnalysisSnapshot) -> Result<(), ApplicationError>;

    async fn save_clarification(
        &self,
        clarification: &ClarificationAnalysis,
    ) -> Result<(), ApplicationError>;

    async fn find_open_clarification(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<ClarificationAnalysis>, ApplicationError>;

    async fn append_clarification_answer(
        &self,
        answer: &ClarificationAnswerRequest,
        snapshot: &AnalysisSnapshot,
    ) -> Result<(), ApplicationError>;

    async fn append_correction(
        &self,
        request: &CorrectionRequest,
        snapshot: &AnalysisSnapshot,
    ) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait AnalysisSnapshotReader: Send + Sync {
    async fn find(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisSnapshot>, ApplicationError>;

    async fn find_revision(
        &self,
        analysis_id: AnalysisId,
        revision_number: u32,
    ) -> Result<Option<serde_json::Value>, ApplicationError>;

    async fn current_revision_id(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisRevisionId>, ApplicationError>;

    async fn list(
        &self,
        user_id: domain::UserId,
        query: AnalysisListQuery,
    ) -> Result<Vec<AnalysisListEntry>, ApplicationError>;

    async fn workflow(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisWorkflow>, ApplicationError>;
}
