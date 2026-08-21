use jsonwebtoken::jwk::{Jwk, JwkSet, KeyAlgorithm};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use persistence_postgres::PostgresAnalysisRepository;
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    env,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tracing::debug;

mod claims;
mod config;
mod error;
mod jwks;
mod subject;
mod verifier;

pub(crate) use claims::*;
pub(crate) use config::*;
pub(crate) use error::*;
pub(crate) use jwks::*;
pub use subject::Authenticator;
pub(crate) use subject::bearer_token;
pub(crate) use verifier::*;

#[cfg(test)]
mod tests;
