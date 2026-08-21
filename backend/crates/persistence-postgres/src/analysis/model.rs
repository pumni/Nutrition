//! Analysis persistence row and mapping responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Clone)]
pub struct PostgresAnalysisRepository {
    pub(crate) pool: PgPool,
}

impl PostgresAnalysisRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
pub(crate) const fn quality_code(value: EvidenceQuality) -> &'static str {
    match value {
        EvidenceQuality::A => "A",
        EvidenceQuality::B => "B",
        EvidenceQuality::C => "C",
        EvidenceQuality::D => "D",
        EvidenceQuality::U => "U",
    }
}

pub(crate) const fn mass_method_code(value: MassResolutionMethod) -> &'static str {
    match value {
        MassResolutionMethod::ExplicitMass => "explicit_mass",
        MassResolutionMethod::VolumeDensity => "volume_density",
        MassResolutionMethod::BrandedServing => "branded_serving",
        MassResolutionMethod::PortionObservation => "portion_observation",
        MassResolutionMethod::CuratedDefault => "curated_default",
    }
}

pub(crate) const fn resolution_status_code(value: MassResolutionMethod) -> &'static str {
    match value {
        MassResolutionMethod::ExplicitMass => "resolved_exact",
        MassResolutionMethod::VolumeDensity
        | MassResolutionMethod::BrandedServing
        | MassResolutionMethod::PortionObservation
        | MassResolutionMethod::CuratedDefault => "resolved_with_assumption",
    }
}

pub(crate) const fn unit_code(value: NutrientUnit) -> &'static str {
    match value {
        NutrientUnit::Kilocalorie => "kcal",
        NutrientUnit::Gram => "g",
        NutrientUnit::Milligram => "mg",
        NutrientUnit::Microgram => "ug",
    }
}

pub(crate) const fn value_status_code(value: ValueStatus) -> &'static str {
    match value {
        ValueStatus::Measured => "measured",
        ValueStatus::Declared => "declared",
        ValueStatus::Calculated => "calculated",
        ValueStatus::Compiled => "compiled",
        ValueStatus::Estimated => "estimated",
        ValueStatus::Trace => "trace",
        ValueStatus::NotDetected => "not_detected",
        ValueStatus::Missing => "missing",
    }
}
