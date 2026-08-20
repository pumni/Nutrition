//! Authentication facade for development bearer and provider-neutral OIDC flows.

mod claims;
mod config;
mod error;
mod implementation;
mod jwks;
mod subject;
mod verifier;

pub use implementation::Authenticator;
