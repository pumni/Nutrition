use crate::{app::AppState, auth::Authenticator};
use adapters::{
    APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS, APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD,
    APPROVED_HOSTED_ENDPOINT, APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES, APPROVED_HOSTED_MODEL,
    APPROVED_HOSTED_PROVIDER, APPROVED_HOSTED_TIMEOUT_MS, ConfiguredMealParser, FixtureParser,
    HOSTED_PROMPT_VERSION, HostedMealParser, HostedParserConfig, PARSER_SCHEMA_VERSION,
};
use application::{AnalysisRevisionService, BehaviorVersions, MealAnalysisService};
use domain::NutrientCode;
use persistence_postgres::{
    PostgresAnalysisRepository, PostgresCatalogEvidenceProvider, PostgresParserTelemetrySink,
    PostgresPortionEvidenceProvider, active_catalog_release_id,
};
use std::{env, net::SocketAddr, str::FromStr, sync::Arc, time::Duration};
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
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
    #[error("{name} must contain at least 32 bytes")]
    InvalidSecretLength { name: &'static str },
    #[error("{name} must be a valid number")]
    InvalidNumeric { name: &'static str },
    #[error("hosted parser configuration is invalid")]
    HostedParser,
}

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("startup configuration is invalid: {0}")]
    Config(#[from] ConfigError),
    #[error("database connection failed")]
    DatabaseConnection,
    #[error("active catalog release is unavailable")]
    ActiveCatalogUnavailable,
    #[error("metrics exporter initialization failed")]
    Metrics,
    #[error("HTTP listener bind failed")]
    HttpListener,
    #[error("HTTP server failed")]
    HttpServer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppEnvironment {
    Local,
    Ci,
    Staging,
    Production,
}

impl AppEnvironment {
    fn from_env() -> Result<Self, ConfigError> {
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

    fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "local" => Ok(Self::Local),
            "ci" => Ok(Self::Ci),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            _ => Err(ConfigError::InvalidEnvironment),
        }
    }

    const fn allows_development_adapters(self) -> bool {
        matches!(self, Self::Local | Self::Ci)
    }
}

/// Loads environment configuration and builds the API dependency graph.
///
/// # Errors
///
/// Returns a typed startup error when configuration is invalid, `PostgreSQL` cannot be connected
/// to, or no active catalog release is available.
pub async fn build() -> Result<(SocketAddr, AppState), StartupError> {
    let environment = AppEnvironment::from_env()?;
    let auth_mode = required_env("AUTH_MODE")?;
    validate_auth_mode(environment, &auth_mode)?;
    let authenticator = Authenticator::from_env(&auth_mode)
        .map_err(|_| ConfigError::InvalidConfiguration { name: "AUTH_MODE" })?;
    let parser_mode = required_env("PARSER_MODE")?;
    validate_parser_mode(environment, &parser_mode)?;
    let bind_addr = match env::var("APP_BIND_ADDR") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
            "127.0.0.1:8080".to_owned()
        }
        Err(env::VarError::NotPresent) => {
            return Err(ConfigError::MissingEnvironment {
                name: "APP_BIND_ADDR",
            }
            .into());
        }
        Err(env::VarError::NotUnicode(_)) => {
            return Err(ConfigError::InvalidUnicode {
                name: "APP_BIND_ADDR",
            }
            .into());
        }
    };
    let address: SocketAddr = bind_addr
        .parse()
        .map_err(|_| ConfigError::InvalidSocketAddress {
            name: "APP_BIND_ADDR",
        })?;
    let database_url = required_env("DATABASE_URL")?;
    let pool = persistence_postgres::connect(&database_url, 8)
        .await
        .map_err(|_| StartupError::DatabaseConnection)?;
    let cursor_hmac_secret = configured_cursor_hmac_secret(environment)?;
    let catalog_started = std::time::Instant::now();
    let catalog_result = active_catalog_release_id(&pool).await;
    metrics::counter!(
        "nutrition_catalog_release_operations_total",
        "operation" => "active_release_lookup",
        "outcome" => if catalog_result.is_ok() { "success" } else { "failure" }
    )
    .increment(1);
    metrics::histogram!(
        "nutrition_catalog_release_operation_duration_seconds",
        "operation" => "active_release_lookup"
    )
    .record(catalog_started.elapsed().as_secs_f64());
    let catalog_release_id = catalog_result.map_err(|_| StartupError::ActiveCatalogUnavailable)?;
    let (parser, prompt_version, model_provider_version) = configured_parser(&pool, &parser_mode)?;
    let versions = BehaviorVersions {
        catalog_release_id,
        parser_schema_version: PARSER_SCHEMA_VERSION.to_owned(),
        prompt_version,
        model_provider_version,
        ..BehaviorVersions::default()
    };
    let repository = PostgresAnalysisRepository::new(pool.clone());
    let food_evidence = PostgresCatalogEvidenceProvider::new(pool.clone());
    let portion_evidence = PostgresPortionEvidenceProvider::new(pool.clone());

    let analyzer = MealAnalysisService::new(
        parser,
        food_evidence.clone(),
        portion_evidence.clone(),
        repository.clone(),
        versions.clone(),
        required_nutrients(),
    );
    let revision_service = Arc::new(AnalysisRevisionService::new(
        food_evidence,
        portion_evidence,
        repository.clone(),
        versions,
        required_nutrients(),
    ));
    let state = AppState {
        authenticator,
        analyzer: Arc::new(analyzer),
        clarification: revision_service.clone(),
        correction: revision_service,
        reader: Arc::new(repository.clone()),
        repository,
        pool,
        cursor_hmac_secret: Arc::new(cursor_hmac_secret),
    };
    Ok((address, state))
}

/// Returns the configured internal metrics listener address.
///
/// # Errors
///
/// Returns a configuration error when the environment requires an address and it is missing or invalid.
#[must_use = "handle the metrics bind address result"]
pub fn metrics_bind_addr() -> Result<SocketAddr, ConfigError> {
    let environment = AppEnvironment::from_env()?;
    let bind_addr = match env::var("API_METRICS_BIND_ADDR") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
            "127.0.0.1:9090".to_owned()
        }
        Err(env::VarError::NotPresent) => {
            return Err(ConfigError::MissingEnvironment {
                name: "API_METRICS_BIND_ADDR",
            });
        }
        Err(env::VarError::NotUnicode(_)) => {
            return Err(ConfigError::InvalidUnicode {
                name: "API_METRICS_BIND_ADDR",
            });
        }
    };
    bind_addr
        .parse()
        .map_err(|_| ConfigError::InvalidSocketAddress {
            name: "API_METRICS_BIND_ADDR",
        })
}

fn configured_cursor_hmac_secret(environment: AppEnvironment) -> Result<Vec<u8>, ConfigError> {
    match env::var("API_CURSOR_HMAC_SECRET") {
        Ok(value) if value.len() >= 32 => Ok(value.into_bytes()),
        Ok(_) => Err(ConfigError::InvalidSecretLength {
            name: "API_CURSOR_HMAC_SECRET",
        }),
        Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
            Ok(b"ci-only-api-cursor-hmac-secret-v1-not-for-deployment".to_vec())
        }
        Err(env::VarError::NotPresent) => Err(ConfigError::MissingEnvironment {
            name: "API_CURSOR_HMAC_SECRET",
        }),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidUnicode {
            name: "API_CURSOR_HMAC_SECRET",
        }),
    }
}

fn required_nutrients() -> Vec<NutrientCode> {
    ["energy_kcal", "protein_g", "carbohydrate_g", "fat_g"]
        .into_iter()
        .map(|code| NutrientCode::new(code).expect("built-in nutrient code must be valid"))
        .collect()
}

fn validate_auth_mode(environment: AppEnvironment, auth_mode: &str) -> Result<(), ConfigError> {
    if (auth_mode == "oidc")
        || (auth_mode == "development" && environment.allows_development_adapters())
    {
        Ok(())
    } else {
        Err(ConfigError::InvalidConfiguration { name: "AUTH_MODE" })
    }
}

fn validate_parser_mode(environment: AppEnvironment, parser_mode: &str) -> Result<(), ConfigError> {
    if (parser_mode == "hosted")
        || (parser_mode == "fixture" && environment.allows_development_adapters())
    {
        Ok(())
    } else {
        Err(ConfigError::InvalidConfiguration {
            name: "PARSER_MODE",
        })
    }
}

fn configured_parser(
    pool: &sqlx::PgPool,
    parser_mode: &str,
) -> Result<(ConfiguredMealParser, String, String), ConfigError> {
    match parser_mode {
        "fixture" => Ok((
            ConfiguredMealParser::Fixture(FixtureParser),
            "fixture-parser-0.2.0".to_owned(),
            "fixture/local".to_owned(),
        )),
        "hosted" => {
            let provider = required_env("LLM_PROVIDER")?;
            let model = required_env("LLM_MODEL")?;
            let endpoint = required_env("LLM_ENDPOINT")?;
            if provider != APPROVED_HOSTED_PROVIDER
                || model != APPROVED_HOSTED_MODEL
                || endpoint != APPROVED_HOSTED_ENDPOINT
            {
                return Err(ConfigError::HostedParser);
            }
            let timeout_ms = environment_number("LLM_TIMEOUT_MS", APPROVED_HOSTED_TIMEOUT_MS)?;
            let maximum_response_bytes = environment_number(
                "LLM_MAXIMUM_RESPONSE_BYTES",
                APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES,
            )?;
            let circuit_failure_threshold = environment_number(
                "LLM_CIRCUIT_FAILURE_THRESHOLD",
                APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD,
            )?;
            let circuit_cooldown_seconds = environment_number(
                "LLM_CIRCUIT_COOLDOWN_SECONDS",
                APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS,
            )?;
            if timeout_ms != APPROVED_HOSTED_TIMEOUT_MS
                || maximum_response_bytes != APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES
                || circuit_failure_threshold != APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD
                || circuit_cooldown_seconds != APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS
            {
                return Err(ConfigError::HostedParser);
            }
            let config = HostedParserConfig {
                endpoint,
                api_key: required_env("LLM_API_KEY")?,
                provider: provider.clone(),
                model: model.clone(),
                timeout: Duration::from_millis(timeout_ms),
                maximum_response_bytes,
                circuit_failure_threshold,
                circuit_cooldown: Duration::from_secs(circuit_cooldown_seconds),
            };
            let parser = HostedMealParser::with_reqwest(config)
                .map_err(|_| ConfigError::HostedParser)?
                .with_telemetry(Arc::new(PostgresParserTelemetrySink::new(pool.clone())));
            Ok((
                ConfiguredMealParser::Hosted(Box::new(parser)),
                HOSTED_PROMPT_VERSION.to_owned(),
                format!("{provider}/{model}"),
            ))
        }
        _ => Err(ConfigError::InvalidConfiguration {
            name: "PARSER_MODE",
        }),
    }
}

fn required_env(name: &'static str) -> Result<String, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) | Err(env::VarError::NotPresent) => Err(ConfigError::MissingEnvironment { name }),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidUnicode { name }),
    }
}

fn environment_number<T>(name: &'static str, default: T) -> Result<T, ConfigError>
where
    T: FromStr + Copy,
{
    match env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|_| ConfigError::InvalidNumeric { name }),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidUnicode { name }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppEnvironment, ConfigError, configured_cursor_hmac_secret, validate_auth_mode,
        validate_parser_mode,
    };

    #[test]
    fn environment_policy_is_explicit() {
        assert_eq!(AppEnvironment::parse("local"), Ok(AppEnvironment::Local));
        assert_eq!(AppEnvironment::parse("ci"), Ok(AppEnvironment::Ci));
        assert_eq!(
            AppEnvironment::parse("staging"),
            Ok(AppEnvironment::Staging)
        );
        assert_eq!(
            AppEnvironment::parse("production"),
            Ok(AppEnvironment::Production)
        );
        assert!(AppEnvironment::Local.allows_development_adapters());
        assert!(AppEnvironment::Ci.allows_development_adapters());
        assert!(!AppEnvironment::Staging.allows_development_adapters());
        assert!(!AppEnvironment::Production.allows_development_adapters());
        assert_eq!(
            AppEnvironment::parse("prod"),
            Err(ConfigError::InvalidEnvironment)
        );
    }

    #[test]
    fn development_adapters_are_non_production_only() {
        assert!(validate_auth_mode(AppEnvironment::Local, "development").is_ok());
        assert!(validate_auth_mode(AppEnvironment::Ci, "development").is_ok());
        assert!(validate_auth_mode(AppEnvironment::Staging, "development").is_err());
        assert!(validate_auth_mode(AppEnvironment::Production, "development").is_err());
        assert!(validate_auth_mode(AppEnvironment::Production, "oidc").is_ok());
        assert!(validate_parser_mode(AppEnvironment::Local, "fixture").is_ok());
        assert!(validate_parser_mode(AppEnvironment::Ci, "fixture").is_ok());
        assert!(validate_parser_mode(AppEnvironment::Staging, "fixture").is_err());
        assert!(validate_parser_mode(AppEnvironment::Production, "fixture").is_err());
        assert!(validate_parser_mode(AppEnvironment::Production, "hosted").is_ok());
    }

    #[test]
    fn cursor_secret_requires_deployment_configuration() {
        assert!(
            configured_cursor_hmac_secret(AppEnvironment::Ci)
                .expect("CI has a cursor secret")
                .len()
                >= 32
        );
        assert_eq!(
            configured_cursor_hmac_secret(AppEnvironment::Production),
            Err(ConfigError::MissingEnvironment {
                name: "API_CURSOR_HMAC_SECRET"
            })
        );
    }
}
