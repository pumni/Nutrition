mod calculation;
mod ids;
mod nutrition;

pub use calculation::{
    CalculationError, CalculationInput, CalculationOperation, CalculationResult,
    DeterministicCalculator, ItemCalculationResult, NutrientCalculationResult, TotalNutrientResult,
};
pub use ids::{
    AnalysisId, AnalysisItemId, AnalysisRevisionId, CatalogReleaseId, CompositionProfileId, FoodId,
    NutrientId, PortionObservationId, RecipeVersionId,
};
pub use nutrition::{
    CompositionSnapshot, CompositionValue, EvidenceQuality, MassEstimate, MassResolutionMethod,
    NutrientCode, NutrientUnit, ResolvedItemInput, ValueStatus,
};

pub const CALCULATION_ENGINE_VERSION: &str = "calc-0.1.0";
