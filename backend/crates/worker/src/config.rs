use metrics_exporter_prometheus::PrometheusBuilder;
use std::{env, net::SocketAddr, time::Duration};

use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub(crate) enum ConfigError {
    #[error("{name} is required")]
    MissingEnvironment { name: &'static str },
    #[error("{name} must be valid Unicode")]
    InvalidUnicode { name: &'static str },
    #[error("APP_ENV is invalid")]
    InvalidEnvironment,
    #[error("{name} configuration is invalid")]
    InvalidConfiguration { name: &'static str },
    #[error("{name} must be a valid socket address")]
    InvalidSocketAddress { name: &'static str },
    #[error("{name} must be an integer")]
    InvalidNumeric { name: &'static str },
    #[error("{name} must be between {minimum} and {maximum}")]
    InvalidNumericBounds {
        name: &'static str,
        minimum: u32,
        maximum: u32,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppEnvironment {
    Local,
    Ci,
    Staging,
    Production,
}

impl AppEnvironment {
    pub(crate) fn from_env() -> Result<Self, ConfigError> {
        match env::var("APP_ENV") {
            Ok(value) => Self::parse(&value),
            Err(env::VarError::NotPresent) => {
                Err(ConfigError::MissingEnvironment { name: "APP_ENV" })
            }
            Err(env::VarError::NotUnicode(_)) => {
                Err(ConfigError::InvalidUnicode { name: "APP_ENV" })
            }
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "local" => Ok(Self::Local),
            "ci" => Ok(Self::Ci),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            _ => Err(ConfigError::InvalidEnvironment),
        }
    }

    pub(crate) const fn allows_development_adapters(self) -> bool {
        matches!(self, Self::Local | Self::Ci)
    }

    pub(crate) const fn allows_source_import(self) -> bool {
        matches!(self, Self::Local | Self::Ci | Self::Staging)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum WorkerMode {
    Idle,
    RunOnce,
    Loop,
}

pub(crate) struct WorkerConfig {
    pub(crate) database_url: String,
    pub(crate) database_pool_size: u32,
    pub(crate) worker_id: String,
    pub(crate) batch_size: i64,
    pub(crate) poll_interval: Duration,
    pub(crate) mode: WorkerMode,
    pub(crate) metrics_bind_addr: Option<SocketAddr>,
}

impl WorkerConfig {
    pub(crate) fn from_env(environment: AppEnvironment) -> Result<Self, ConfigError> {
        let mode = match env::var("WORKER_MODE").as_deref() {
            Ok("run-once") => WorkerMode::RunOnce,
            Ok("loop") => WorkerMode::Loop,
            Ok("idle") => WorkerMode::Idle,
            Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
                WorkerMode::Idle
            }
            Err(env::VarError::NotPresent) => {
                return Err(ConfigError::MissingEnvironment {
                    name: "WORKER_MODE",
                });
            }
            Err(env::VarError::NotUnicode(_)) => {
                return Err(ConfigError::InvalidUnicode {
                    name: "WORKER_MODE",
                });
            }
            Ok(_) => {
                return Err(ConfigError::InvalidConfiguration {
                    name: "WORKER_MODE",
                });
            }
        };
        let worker_id = match env::var("WORKER_ID") {
            Ok(value) if !value.trim().is_empty() => value,
            Ok(_) => {
                return Err(ConfigError::InvalidConfiguration { name: "WORKER_ID" });
            }
            Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
                "worker-local".to_owned()
            }
            Err(env::VarError::NotPresent) => {
                return Err(ConfigError::MissingEnvironment { name: "WORKER_ID" });
            }
            Err(env::VarError::NotUnicode(_)) => {
                return Err(ConfigError::InvalidUnicode { name: "WORKER_ID" });
            }
        };
        Ok(Self {
            database_url: required_env("DATABASE_URL")?,
            database_pool_size: env_u32("WORKER_DATABASE_POOL_SIZE", 4, 1, 32)?,
            worker_id,
            batch_size: i64::from(env_u32("WORKER_BATCH_SIZE", 20, 1, 100)?),
            poll_interval: Duration::from_millis(u64::from(env_u32(
                "WORKER_POLL_INTERVAL_MS",
                500,
                50,
                60_000,
            )?)),
            mode,
            metrics_bind_addr: configured_metrics_bind_addr(environment)?,
        })
    }
}

fn configured_metrics_bind_addr(
    environment: AppEnvironment,
) -> Result<Option<SocketAddr>, ConfigError> {
    match env::var("WORKER_METRICS_BIND_ADDR") {
        Ok(value) => value
            .parse()
            .map(Some)
            .map_err(|_| ConfigError::InvalidSocketAddress {
                name: "WORKER_METRICS_BIND_ADDR",
            }),
        Err(env::VarError::NotPresent) if environment.allows_development_adapters() => Ok(None),
        Err(env::VarError::NotPresent) => Err(ConfigError::MissingEnvironment {
            name: "WORKER_METRICS_BIND_ADDR",
        }),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidUnicode {
            name: "WORKER_METRICS_BIND_ADDR",
        }),
    }
}

pub(crate) fn initialize_metrics(address: Option<SocketAddr>) -> Result<(), ()> {
    let buckets = [
        0.005, 0.01, 0.025, 0.05, 0.1, 0.2, 0.3, 0.5, 0.75, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0, 15.0,
        30.0,
    ];
    let builder = PrometheusBuilder::new()
        .set_buckets(&buckets)
        .map_err(|_| ())?;
    if let Some(address) = address {
        builder
            .with_http_listener(address)
            .install()
            .map_err(|_| ())
    } else {
        builder.install_recorder().map(|_| ()).map_err(|_| ())
    }
}

fn required_env(name: &'static str) -> Result<String, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) | Err(env::VarError::NotPresent) => Err(ConfigError::MissingEnvironment { name }),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidUnicode { name }),
    }
}

fn env_u32(
    name: &'static str,
    default: u32,
    minimum: u32,
    maximum: u32,
) -> Result<u32, ConfigError> {
    let value = match env::var(name) {
        Ok(raw) => raw
            .parse()
            .map_err(|_| ConfigError::InvalidNumeric { name })?,
        Err(env::VarError::NotPresent) => default,
        Err(env::VarError::NotUnicode(_)) => {
            return Err(ConfigError::InvalidUnicode { name });
        }
    };
    validate_u32(name, value, minimum, maximum)
}

pub(crate) fn validate_u32(
    name: &'static str,
    value: u32,
    minimum: u32,
    maximum: u32,
) -> Result<u32, ConfigError> {
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(ConfigError::InvalidNumericBounds {
            name,
            minimum,
            maximum,
        })
    }
}
