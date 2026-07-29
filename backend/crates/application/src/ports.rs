use crate::{
    AnalysisSnapshot, ParseRequest, ParsedMealDocument, ParsedMealItem, ResolvedFoodEvidence,
    ResolvedPortionEvidence,
};
use async_trait::async_trait;
use domain::{AnalysisId, FoodId};
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
    #[error("analysis was not found")]
    NotFound,
}

#[async_trait]
pub trait MealTextParser: Send + Sync {
    async fn parse(&self, request: ParseRequest) -> Result<ParsedMealDocument, ApplicationError>;
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
}

#[async_trait]
pub trait AnalysisRepository: Send + Sync {
    async fn save(&self, snapshot: &AnalysisSnapshot) -> Result<(), ApplicationError>;
}

#[async_trait]
pub trait AnalysisSnapshotReader: Send + Sync {
    async fn find(
        &self,
        analysis_id: AnalysisId,
    ) -> Result<Option<AnalysisSnapshot>, ApplicationError>;
}
