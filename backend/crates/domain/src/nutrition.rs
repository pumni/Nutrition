use crate::{CompositionProfileId, FoodId, PortionObservationId, RecipeVersionId};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct NutrientCode(String);

impl NutrientCode {
    /// Creates a canonical lowercase snake-case nutrient code.
    ///
    /// # Errors
    ///
    /// Returns [`NutrientCodeError`] when the value is empty, longer than 64 bytes, or contains
    /// characters outside lowercase ASCII letters, digits, and underscores.
    pub fn new(value: impl Into<String>) -> Result<Self, NutrientCodeError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        if valid {
            Ok(Self(value))
        } else {
            Err(NutrientCodeError)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NutrientCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NutrientCodeError;

impl fmt::Display for NutrientCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("nutrient code must be lowercase snake_case and at most 64 bytes")
    }
}

impl std::error::Error for NutrientCodeError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NutrientUnit {
    Kilocalorie,
    Gram,
    Milligram,
    Microgram,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueStatus {
    Measured,
    Declared,
    Calculated,
    Compiled,
    Estimated,
    Trace,
    NotDetected,
    Missing,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EvidenceQuality {
    A,
    B,
    C,
    D,
    U,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MassResolutionMethod {
    ExplicitMass,
    VolumeDensity,
    BrandedServing,
    PortionObservation,
    CuratedDefault,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MassEstimate {
    pub central_g: Decimal,
    pub lower_g: Option<Decimal>,
    pub upper_g: Option<Decimal>,
    pub evidence_id: Option<PortionObservationId>,
    pub method: MassResolutionMethod,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompositionValue {
    pub nutrient: NutrientCode,
    pub amount: Option<Decimal>,
    pub lower_amount: Option<Decimal>,
    pub upper_amount: Option<Decimal>,
    pub unit: NutrientUnit,
    pub status: ValueStatus,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompositionSnapshot {
    pub profile_id: CompositionProfileId,
    pub basis_g: Decimal,
    pub quality: EvidenceQuality,
    pub values: Vec<CompositionValue>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResolvedItemInput {
    pub food_id: FoodId,
    pub mass: MassEstimate,
    pub composition: CompositionSnapshot,
    pub recipe_version_id: Option<RecipeVersionId>,
}
