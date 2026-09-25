//! Static registry for installed structured-model providers.

use crate::{
    APPROVED_HOSTED_ENDPOINT, APPROVED_HOSTED_MODEL, HostedParserConfig, ModelIdentity,
    OpenAiResponsesProvider, StructuredModel,
};
use std::{fmt, sync::Arc};

/// Typed provider key accepted by the server-side configuration boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderId {
    OpenAi,
}

impl ProviderId {
    /// Parses a configured provider key against the installed provider IDs.
    ///
    /// # Errors
    ///
    /// Returns `UnknownProvider` when the key is not installed.
    pub fn parse(value: &str) -> Result<Self, ProviderRegistryError> {
        match value {
            "openai" => Ok(Self::OpenAi),
            _ => Err(ProviderRegistryError::UnknownProvider),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
        }
    }
}

/// Safe, content-free failures from provider selection and construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRegistryError {
    UnknownProvider,
    MissingModel,
    UnsupportedModel,
    MissingSecret,
    InvalidEndpoint,
    InvalidConfiguration,
    InitializationFailed,
}

impl fmt::Display for ProviderRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnknownProvider => "unknown model provider",
            Self::MissingModel => "model is required",
            Self::UnsupportedModel => "model is not installed for this provider",
            Self::MissingSecret => "provider secret is required",
            Self::InvalidEndpoint => "provider endpoint is invalid",
            Self::InvalidConfiguration => "provider configuration is invalid",
            Self::InitializationFailed => "provider initialization failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProviderRegistryError {}

/// A successfully resolved provider and model, plus the selected SPI implementation.
pub struct ProviderSelection {
    provider: ProviderId,
    model: ModelIdentity,
    implementation: Arc<dyn StructuredModel>,
}

impl ProviderSelection {
    #[must_use]
    pub const fn provider_id(&self) -> ProviderId {
        self.provider
    }

    #[must_use]
    pub const fn model(&self) -> &ModelIdentity {
        &self.model
    }

    /// Returns the exact provider/model value suitable for behavior version recording.
    #[must_use]
    pub fn behavior_version(&self) -> String {
        format!("{}/{}", self.provider.as_str(), self.model.as_str())
    }

    #[must_use]
    pub fn structured_model(&self) -> Arc<dyn StructuredModel> {
        Arc::clone(&self.implementation)
    }
}

/// Resolves configured provider/model pairs against the providers installed in this binary.
#[derive(Clone, Copy, Debug, Default)]
pub struct StructuredModelProviderRegistry;

impl StructuredModelProviderRegistry {
    /// Validates server configuration and constructs the selected installed provider.
    ///
    /// Registry entries contain only public identity, model, and endpoint metadata. The API key
    /// is read only from the supplied runtime config and is passed directly to provider creation.
    ///
    /// # Errors
    ///
    /// Returns a content-free error when the provider, model, secret, endpoint, or provider
    /// construction is invalid.
    pub fn select(
        &self,
        config: &HostedParserConfig,
    ) -> Result<ProviderSelection, ProviderRegistryError> {
        let provider = ProviderId::parse(&config.provider)?;
        let registration = PROVIDERS
            .iter()
            .find(|registration| registration.id == provider)
            .ok_or(ProviderRegistryError::UnknownProvider)?;

        if config.model.trim().is_empty() {
            return Err(ProviderRegistryError::MissingModel);
        }
        if !registration.models.contains(&config.model.as_str()) {
            return Err(ProviderRegistryError::UnsupportedModel);
        }
        if config.api_key.trim().is_empty() {
            return Err(ProviderRegistryError::MissingSecret);
        }
        if config.endpoint != registration.endpoint {
            return Err(ProviderRegistryError::InvalidEndpoint);
        }
        config
            .validate()
            .map_err(|_| ProviderRegistryError::InvalidConfiguration)?;

        let implementation = (registration.construct)(config)?;
        Ok(ProviderSelection {
            provider,
            model: ModelIdentity::new(config.model.clone()),
            implementation,
        })
    }
}

struct ProviderRegistration {
    id: ProviderId,
    models: &'static [&'static str],
    endpoint: &'static str,
    construct: fn(&HostedParserConfig) -> Result<Arc<dyn StructuredModel>, ProviderRegistryError>,
}

static PROVIDERS: [ProviderRegistration; 1] = [ProviderRegistration {
    id: ProviderId::OpenAi,
    models: &[APPROVED_HOSTED_MODEL],
    endpoint: APPROVED_HOSTED_ENDPOINT,
    construct: construct_openai,
}];

fn construct_openai(
    config: &HostedParserConfig,
) -> Result<Arc<dyn StructuredModel>, ProviderRegistryError> {
    OpenAiResponsesProvider::new(config)
        .map(|provider| Arc::new(provider) as Arc<dyn StructuredModel>)
        .map_err(|_| ProviderRegistryError::InitializationFailed)
}

#[cfg(test)]
mod tests {
    use super::{ProviderId, ProviderRegistryError, StructuredModelProviderRegistry};
    use crate::{
        APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS, APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD,
        APPROVED_HOSTED_ENDPOINT, APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES, APPROVED_HOSTED_MODEL,
        APPROVED_HOSTED_PROVIDER, APPROVED_HOSTED_TIMEOUT_MS, HostedParserConfig,
    };
    use std::time::Duration;

    const TEST_SECRET: &str = "registry-test-secret";

    fn config() -> HostedParserConfig {
        HostedParserConfig {
            endpoint: APPROVED_HOSTED_ENDPOINT.to_owned(),
            api_key: TEST_SECRET.to_owned(),
            provider: APPROVED_HOSTED_PROVIDER.to_owned(),
            model: APPROVED_HOSTED_MODEL.to_owned(),
            timeout: Duration::from_millis(APPROVED_HOSTED_TIMEOUT_MS),
            maximum_response_bytes: APPROVED_HOSTED_MAXIMUM_RESPONSE_BYTES,
            circuit_failure_threshold: APPROVED_HOSTED_CIRCUIT_FAILURE_THRESHOLD,
            circuit_cooldown: Duration::from_secs(APPROVED_HOSTED_CIRCUIT_COOLDOWN_SECONDS),
        }
    }

    #[test]
    fn selects_installed_provider_and_model_with_exact_behavior_identity() {
        let selection = StructuredModelProviderRegistry
            .select(&config())
            .expect("approved provider selection");

        assert_eq!(selection.provider_id(), ProviderId::OpenAi);
        assert_eq!(selection.model().as_str(), APPROVED_HOSTED_MODEL);
        assert_eq!(
            selection.behavior_version(),
            format!("{APPROVED_HOSTED_PROVIDER}/{APPROVED_HOSTED_MODEL}")
        );
    }

    #[test]
    fn rejects_unknown_provider_without_disclosing_configuration() {
        let mut config = config();
        config.provider = "unknown-provider".to_owned();

        match StructuredModelProviderRegistry.select(&config) {
            Err(error) => {
                assert_eq!(error, ProviderRegistryError::UnknownProvider);
                assert!(!error.to_string().contains(TEST_SECRET));
            }
            Ok(_) => panic!("unknown providers must fail closed"),
        }
    }

    #[test]
    fn rejects_missing_and_uninstalled_models() {
        let mut missing_model = config();
        missing_model.model.clear();
        assert!(matches!(
            StructuredModelProviderRegistry.select(&missing_model),
            Err(ProviderRegistryError::MissingModel)
        ));

        let mut unsupported_model = config();
        unsupported_model.model = "uninstalled-model".to_owned();
        assert!(matches!(
            StructuredModelProviderRegistry.select(&unsupported_model),
            Err(ProviderRegistryError::UnsupportedModel)
        ));
    }

    #[test]
    fn rejects_missing_secret_and_invalid_endpoint() {
        let mut missing_secret = config();
        missing_secret.api_key.clear();
        assert!(matches!(
            StructuredModelProviderRegistry.select(&missing_secret),
            Err(ProviderRegistryError::MissingSecret)
        ));

        let mut invalid_endpoint = config();
        invalid_endpoint.endpoint = "http://127.0.0.1/responses".to_owned();
        match StructuredModelProviderRegistry.select(&invalid_endpoint) {
            Err(error) => {
                assert_eq!(error, ProviderRegistryError::InvalidEndpoint);
                assert!(!error.to_string().contains(TEST_SECRET));
            }
            Ok(_) => panic!("provider endpoints are fixed and HTTPS-only"),
        }
    }
}
