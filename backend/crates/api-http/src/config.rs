use crate::{app::AppState, oidc::Authenticator};
use adapters::{
    ConfiguredMealParser, FixtureParser, HOSTED_PROMPT_VERSION, HostedMealParser,
    HostedParserConfig, PARSER_SCHEMA_VERSION,
};
use application::{AnalysisRevisionService, BehaviorVersions, MealAnalysisService};
use domain::NutrientCode;
use persistence_postgres::{
    PostgresAnalysisRepository, PostgresCatalogEvidenceProvider, PostgresParserTelemetrySink,
    PostgresPortionEvidenceProvider, active_catalog_release_id,
};
use std::{env, net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppEnvironment {
    Local,
    Ci,
    Staging,
    Production,
}

impl AppEnvironment {
    fn from_env() -> Self {
        let value = env::var("APP_ENV").expect("APP_ENV is required");
        Self::parse(&value).unwrap_or_else(|error| panic!("{error}"))
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "local" => Ok(Self::Local),
            "ci" => Ok(Self::Ci),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            _ => Err("APP_ENV must be local, ci, staging, or production".to_owned()),
        }
    }

    const fn allows_development_adapters(self) -> bool {
        matches!(self, Self::Local | Self::Ci)
    }
}

pub(crate) async fn build() -> (SocketAddr, AppState) {
    let environment = AppEnvironment::from_env();
    let auth_mode = env::var("AUTH_MODE").expect("AUTH_MODE is required");
    validate_auth_mode(environment, &auth_mode).expect("authentication configuration is invalid");
    let authenticator =
        Authenticator::from_env(&auth_mode).expect("authentication configuration is invalid");
    let parser_mode = env::var("PARSER_MODE").expect("PARSER_MODE is required");
    validate_parser_mode(environment, &parser_mode).expect("parser configuration is invalid");
    let bind_addr = match env::var("APP_BIND_ADDR") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) if environment.allows_development_adapters() => {
            "127.0.0.1:8080".to_owned()
        }
        Err(env::VarError::NotPresent) => {
            panic!("APP_BIND_ADDR is required when APP_ENV is staging or production")
        }
        Err(env::VarError::NotUnicode(_)) => panic!("APP_BIND_ADDR must be valid Unicode"),
    };
    let address: SocketAddr = bind_addr
        .parse()
        .expect("APP_BIND_ADDR must be a valid socket address");
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL is required");
    let pool = persistence_postgres::connect(&database_url, 8)
        .await
        .expect("API could not connect to PostgreSQL");
    let catalog_release_id = active_catalog_release_id(&pool)
        .await
        .expect("an active catalog release is required");
    let (parser, prompt_version, model_provider_version) =
        configured_parser(&pool, &parser_mode).expect("parser configuration is invalid");
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
    };
    (address, state)
}

fn required_nutrients() -> Vec<NutrientCode> {
    ["energy_kcal", "protein_g", "carbohydrate_g", "fat_g"]
        .into_iter()
        .map(|code| NutrientCode::new(code).expect("built-in nutrient code must be valid"))
        .collect()
}

fn validate_auth_mode(environment: AppEnvironment, auth_mode: &str) -> Result<(), String> {
    match auth_mode {
        "development" if environment.allows_development_adapters() => Ok(()),
        "development" => Err(
            "AUTH_MODE=development is forbidden when APP_ENV is staging or production".to_owned(),
        ),
        "oidc" => Ok(()),
        _ => Err("AUTH_MODE must be development or oidc".to_owned()),
    }
}

fn validate_parser_mode(environment: AppEnvironment, parser_mode: &str) -> Result<(), String> {
    match parser_mode {
        "fixture" if environment.allows_development_adapters() => Ok(()),
        "fixture" => {
            Err("PARSER_MODE=fixture is forbidden when APP_ENV is staging or production".to_owned())
        }
        "hosted" => Ok(()),
        _ => Err("PARSER_MODE must be fixture or hosted".to_owned()),
    }
}

fn configured_parser(
    pool: &sqlx::PgPool,
    parser_mode: &str,
) -> Result<(ConfiguredMealParser, String, String), String> {
    match parser_mode {
        "fixture" => Ok((
            ConfiguredMealParser::Fixture(FixtureParser),
            "fixture-parser-0.2.0".to_owned(),
            "fixture/local".to_owned(),
        )),
        "hosted" => {
            let provider =
                env::var("LLM_PROVIDER").map_err(|_| "LLM_PROVIDER is required".to_owned())?;
            let model = env::var("LLM_MODEL").map_err(|_| "LLM_MODEL is required".to_owned())?;
            let config = HostedParserConfig {
                endpoint: env::var("LLM_ENDPOINT")
                    .map_err(|_| "LLM_ENDPOINT is required".to_owned())?,
                api_key: env::var("LLM_API_KEY")
                    .map_err(|_| "LLM_API_KEY is required".to_owned())?,
                provider: provider.clone(),
                model: model.clone(),
                timeout: Duration::from_millis(environment_number("LLM_TIMEOUT_MS", 3_000)?),
                maximum_response_bytes: environment_number("LLM_MAXIMUM_RESPONSE_BYTES", 65_536)?,
                circuit_failure_threshold: environment_number("LLM_CIRCUIT_FAILURE_THRESHOLD", 5)?,
                circuit_cooldown: Duration::from_secs(environment_number(
                    "LLM_CIRCUIT_COOLDOWN_SECONDS",
                    30,
                )?),
            };
            let parser = HostedMealParser::with_reqwest(config)
                .map_err(|error| error.to_string())?
                .with_telemetry(Arc::new(PostgresParserTelemetrySink::new(pool.clone())));
            Ok((
                ConfiguredMealParser::Hosted(Box::new(parser)),
                HOSTED_PROMPT_VERSION.to_owned(),
                format!("{provider}/{model}"),
            ))
        }
        _ => Err("PARSER_MODE must be fixture or hosted".to_owned()),
    }
}

fn environment_number<T>(name: &str, default: T) -> Result<T, String>
where
    T: FromStr + Copy,
{
    match env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|_| format!("{name} must be a valid number")),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid Unicode")),
    }
}

#[cfg(test)]
mod tests {
    use super::{AppEnvironment, validate_auth_mode, validate_parser_mode};

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
        assert!(AppEnvironment::parse("prod").is_err());
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
}
