//! Analysis repository facade. The public repository type remains stable while responsibilities
//! are mapped to focused retrieval modules.

mod create;
mod idempotency;
mod implementation;
mod model;
mod ownership;
mod read;
mod revision;
mod snapshot;

pub use implementation::PostgresAnalysisRepository;
