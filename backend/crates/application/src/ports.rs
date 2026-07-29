use crate::{AnalysisSnapshot, ParseRequest, ParsedMealDocument, ParsedMealItem, ResolvedEvidence};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("meal parser unavailable: {0}")]
    ParserUnavailable(String),
    #[error("food or portion evidence is insufficient: {0}")]
    InsufficientEvidence(String),
    #[error("calculation failed: {0}")]
    Calculation(String),
    #[error("analysis persistence failed")]
    Persistence,
}

#[async_trait]
pub trait MealTextParser: Send + Sync {
    async fn parse(&self, request: ParseRequest) -> Result<ParsedMealDocument, ApplicationError>;
}

#[async_trait]
pub trait CatalogEvidenceProvider: Send + Sync {
    async fn resolve_direct(
        &self,
        locale: &str,
        item: &ParsedMealItem,
    ) -> Result<ResolvedEvidence, ApplicationError>;
}

#[async_trait]
pub trait AnalysisRepository: Send + Sync {
    async fn save(&self, snapshot: &AnalysisSnapshot) -> Result<(), ApplicationError>;
}
