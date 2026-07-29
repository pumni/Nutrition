mod calculation;
mod ids;
mod nutrition;

pub use calculation::{
    CalculationError, CalculationInput, CalculationOperation, CalculationResult,
    DeterministicCalculator, ItemCalculationResult, NutrientCalculationResult,
};
pub use ids::{
    AnalysisId, AnalysisRevisionId, CompositionProfileId, FoodId, PortionObservationId,
    RecipeVersionId,
};
pub use nutrition::{
    CompositionSnapshot, CompositionValue, EvidenceQuality, MassEstimate, MassResolutionMethod,
    NutrientCode, NutrientUnit, ResolvedItemInput, ValueStatus,
};

pub const CALCULATION_ENGINE_VERSION: &str = "calc-0.1.0";
