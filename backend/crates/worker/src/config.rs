use metrics_exporter_prometheus::PrometheusBuilder;
use std::{env, net::SocketAddr, time::Duration};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AppEnvironment {
    Local,
    Ci,
    Staging,
    Production,
}

impl AppEnvironment {
    pub(crate) fn from_env() -> Self {
        let value = env::var("APP_ENV").expect("APP_ENV is required");
        Self::parse(&value).unwrap_or_else(|error| panic!("{error}"))
    }

    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "local" => Ok(Self::Local),
            "ci" => Ok(Self::Ci),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            _ => Err("APP_ENV must be local, ci, staging, or production".to_owned()),
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
    pub(crate) fn from_env(environment: AppEnvironment) -> Self {
        let mode = match env::var("WORKER_MODE").as_deref() {
            Ok("run-once") => WorkerMode::RunOnce,
            Ok("loop") => WorkerMode::Loop,
            Ok("idle") => WorkerMode::Idle,
            Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
                WorkerMode::Idle
            }
            Err(env::VarError::NotPresent) => {
                panic!("WORKER_MODE is required when APP_ENV is staging or production")
            }
            Err(env::VarError::NotUnicode(_)) => panic!("WORKER_MODE must be valid Unicode"),
            Ok(value) => panic!("unsupported WORKER_MODE: {value}"),
        };
        let worker_id = match env::var("WORKER_ID") {
            Ok(value) if !value.trim().is_empty() => value,
            Ok(_) => panic!("WORKER_ID must not be empty"),
            Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
                "worker-local".to_owned()
            }
            Err(env::VarError::NotPresent) => {
                panic!("WORKER_ID is required when APP_ENV is staging or production")
            }
            Err(env::VarError::NotUnicode(_)) => panic!("WORKER_ID must be valid Unicode"),
        };
        Self {
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL is required for the worker process"),
            database_pool_size: env_u32("WORKER_DATABASE_POOL_SIZE", 4, 1, 32),
            worker_id,
            batch_size: i64::from(env_u32("WORKER_BATCH_SIZE", 20, 1, 100)),
            poll_interval: Duration::from_millis(u64::from(env_u32(
                "WORKER_POLL_INTERVAL_MS",
                500,
                50,
                60_000,
            ))),
            mode,
            metrics_bind_addr: configured_metrics_bind_addr(environment),
        }
    }
}

fn configured_metrics_bind_addr(environment: AppEnvironment) -> Option<SocketAddr> {
    match env::var("WORKER_METRICS_BIND_ADDR") {
        Ok(value) => Some(
            value
                .parse()
                .expect("WORKER_METRICS_BIND_ADDR must be a valid socket address"),
        ),
        Err(env::VarError::NotPresent) if environment.allows_development_adapters() => None,
        Err(env::VarError::NotPresent) => {
            panic!("WORKER_METRICS_BIND_ADDR is required when APP_ENV is staging or production")
        }
        Err(env::VarError::NotUnicode(_)) => {
            panic!("WORKER_METRICS_BIND_ADDR must be valid Unicode")
        }
    }
}

pub(crate) fn initialize_metrics(address: Option<SocketAddr>) {
    let buckets = [
        0.005, 0.01, 0.025, 0.05, 0.1, 0.2, 0.3, 0.5, 0.75, 1.0, 2.0, 4.0, 6.0, 8.0, 10.0, 15.0,
        30.0,
    ];
    let builder = PrometheusBuilder::new()
        .set_buckets(&buckets)
        .expect("default Prometheus histogram buckets are valid");
    if let Some(address) = address {
        builder
            .with_http_listener(address)
            .install()
            .expect("failed to install Prometheus metrics exporter");
    } else {
        builder
            .install_recorder()
            .expect("failed to install Prometheus metrics recorder");
    }
}

fn env_u32(name: &str, default: u32, minimum: u32, maximum: u32) -> u32 {
    let value = env::var(name).map_or(default, |raw| {
        raw.parse()
            .unwrap_or_else(|_| panic!("{name} must be an integer"))
    });
    assert!(
        (minimum..=maximum).contains(&value),
        "{name} must be between {minimum} and {maximum}"
    );
    value
}
