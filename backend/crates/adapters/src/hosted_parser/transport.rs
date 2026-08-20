//! HTTPS, timeout, response-byte bound, retry, and redirect responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

#[async_trait]
pub trait HostedLlmTransport: Send + Sync {
    async fn complete(
        &self,
        request: &ProviderRequest,
        maximum_response_bytes: usize,
    ) -> Result<ProviderResponse, TransportError>;
}

#[derive(Clone)]
pub struct ReqwestHostedLlmTransport {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
}

impl ReqwestHostedLlmTransport {
    /// Creates a TLS-only hosted transport.
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` when the endpoint is not HTTPS or the client cannot be built.
    pub fn new(config: &HostedParserConfig) -> Result<Self, ApplicationError> {
        config.validate()?;
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ApplicationError::InvalidInput("HTTP client setup failed".to_owned()))?;
        Ok(Self {
            client,
            endpoint: config.endpoint.clone(),
            api_key: config.api_key.clone(),
        })
    }
}

#[async_trait]
impl HostedLlmTransport for ReqwestHostedLlmTransport {
    async fn complete(
        &self,
        request: &ProviderRequest,
        maximum_response_bytes: usize,
    ) -> Result<ProviderResponse, TransportError> {
        let body = openai_responses_request(request);
        let mut response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| classify_reqwest_error(&error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(TransportError {
                kind: if status.as_u16() == 429 || status.is_server_error() {
                    TransportErrorKind::Transient
                } else {
                    TransportErrorKind::Permanent
                },
                code: format!("provider_http_{}", status.as_u16()),
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > maximum_response_bytes as u64)
        {
            return Err(TransportError {
                kind: TransportErrorKind::Permanent,
                code: "provider_response_too_large".to_owned(),
            });
        }
        let mut bytes = Vec::with_capacity(
            response
                .content_length()
                .and_then(|length| usize::try_from(length).ok())
                .unwrap_or(0)
                .min(maximum_response_bytes),
        );
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| classify_reqwest_error(&error))?
        {
            if bytes.len().saturating_add(chunk.len()) > maximum_response_bytes {
                return Err(TransportError {
                    kind: TransportErrorKind::Permanent,
                    code: "provider_response_too_large".to_owned(),
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        parse_openai_response(&bytes)
    }
}
