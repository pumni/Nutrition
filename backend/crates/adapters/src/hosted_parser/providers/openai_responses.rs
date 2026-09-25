//! `OpenAI` Responses API implementation of the structured-generation SPI.

use super::super::HostedParserConfig;
use crate::{
    StructuredGenerationRequest, StructuredGenerationResponse, StructuredModel,
    StructuredModelError, StructuredModelErrorClassification, StructuredResponseMetadata,
};
use application::ApplicationError;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Clone)]
pub struct OpenAiResponsesProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
}

impl OpenAiResponsesProvider {
    /// Creates a bounded `OpenAI` Responses API provider.
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` when configuration is unsafe or the HTTP client cannot be built.
    pub fn new(config: &HostedParserConfig) -> Result<Self, ApplicationError> {
        config.validate()?;
        let client = build_http_client(config.timeout)
            .map_err(|_| ApplicationError::InvalidInput("HTTP client setup failed".to_owned()))?;
        Ok(Self {
            client,
            endpoint: config.endpoint.clone(),
            api_key: config.api_key.clone(),
        })
    }
}

#[async_trait]
impl StructuredModel for OpenAiResponsesProvider {
    async fn generate(
        &self,
        request: &StructuredGenerationRequest,
        maximum_response_bytes: usize,
    ) -> Result<StructuredGenerationResponse, StructuredModelError> {
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
            return Err(classify_http_status(status));
        }

        let content_length = response.content_length();
        if declared_response_is_too_large(content_length, maximum_response_bytes) {
            return Err(response_too_large());
        }
        let mut bytes = Vec::with_capacity(
            content_length
                .and_then(|length| usize::try_from(length).ok())
                .unwrap_or(0)
                .min(maximum_response_bytes),
        );
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| classify_reqwest_error(&error))?
        {
            append_response_chunk(&mut bytes, &chunk, maximum_response_bytes)?;
        }
        parse_openai_response(&bytes)
    }
}

fn build_http_client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

fn openai_responses_request(request: &StructuredGenerationRequest) -> Value {
    json!({
        "model": request.model.as_str(),
        "instructions": request.system_instruction,
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": request.untrusted_input.as_str()
            }]
        }],
        "text": {
            "format": {
                "type": "json_schema",
                "name": "parsed_meal",
                "schema": request.schema.value(),
                "strict": true
            }
        },
        "store": false
    })
}

fn classify_http_status(status: reqwest::StatusCode) -> StructuredModelError {
    StructuredModelError::new(
        if status.as_u16() == 429 || status.is_server_error() {
            StructuredModelErrorClassification::Transient
        } else {
            StructuredModelErrorClassification::Permanent
        },
        format!("provider_http_{}", status.as_u16()),
    )
}

fn classify_reqwest_error(error: &reqwest::Error) -> StructuredModelError {
    StructuredModelError::new(
        if error.is_timeout() || error.is_connect() {
            StructuredModelErrorClassification::Transient
        } else {
            StructuredModelErrorClassification::Permanent
        },
        if error.is_timeout() {
            "provider_timeout"
        } else {
            "provider_transport_error"
        },
    )
}

fn declared_response_is_too_large(
    content_length: Option<u64>,
    maximum_response_bytes: usize,
) -> bool {
    content_length.is_some_and(|length| length > maximum_response_bytes as u64)
}

fn append_response_chunk(
    bytes: &mut Vec<u8>,
    chunk: &[u8],
    maximum_response_bytes: usize,
) -> Result<(), StructuredModelError> {
    if bytes.len().saturating_add(chunk.len()) > maximum_response_bytes {
        return Err(response_too_large());
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

fn response_too_large() -> StructuredModelError {
    StructuredModelError::new(
        StructuredModelErrorClassification::Permanent,
        "provider_response_too_large",
    )
}

#[derive(Deserialize)]
struct OpenAiResponseEnvelope {
    output: Vec<OpenAiOutputItem>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiOutputItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    content: Vec<OpenAiOutputContent>,
}

#[derive(Deserialize)]
struct OpenAiOutputContent {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    input_tokens: Option<i64>,
    #[serde(default)]
    output_tokens: Option<i64>,
}

fn parse_openai_response(
    bytes: &[u8],
) -> Result<StructuredGenerationResponse, StructuredModelError> {
    let response: OpenAiResponseEnvelope = serde_json::from_slice(bytes).map_err(|_| {
        StructuredModelError::new(
            StructuredModelErrorClassification::Permanent,
            "provider_envelope_invalid",
        )
    })?;
    let text_outputs = response
        .output
        .iter()
        .filter(|item| item.item_type == "message")
        .flat_map(|item| item.content.iter())
        .filter(|content| content.content_type == "output_text")
        .filter_map(|content| content.text.as_deref())
        .collect::<Vec<_>>();
    let output = if text_outputs.len() == 1 {
        serde_json::from_str(text_outputs[0]).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    Ok(StructuredGenerationResponse {
        output,
        metadata: StructuredResponseMetadata {
            input_tokens: response.usage.as_ref().and_then(|usage| usage.input_tokens),
            output_tokens: response
                .usage
                .as_ref()
                .and_then(|usage| usage.output_tokens),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ModelIdentity, ProviderIdentity, StrictJsonSchema, StructuredGenerationRequest,
        UntrustedInput,
    };
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc::{self, Receiver},
        thread::{self, JoinHandle},
    };

    const TEST_API_KEY: &str = "test-secret-token";

    fn config() -> HostedParserConfig {
        HostedParserConfig {
            endpoint: "https://api.openai.com/v1/responses".to_owned(),
            api_key: TEST_API_KEY.to_owned(),
            provider: "openai".to_owned(),
            model: "gpt-5.6-luna".to_owned(),
            timeout: Duration::from_secs(2),
            maximum_response_bytes: 16_384,
            circuit_failure_threshold: 5,
            circuit_cooldown: Duration::from_secs(30),
        }
    }

    fn generation_request() -> StructuredGenerationRequest {
        StructuredGenerationRequest {
            provider: ProviderIdentity::new("openai"),
            model: ModelIdentity::new("gpt-5.6-luna"),
            system_instruction: "Extract only supported facts.".to_owned(),
            untrusted_input: UntrustedInput::new("locale: vi-VN\nmeal: 2 quả trứng luộc"),
            schema: StrictJsonSchema::new(json!({
                "type": "object",
                "properties": {"ok": {"type": "boolean"}},
                "required": ["ok"],
                "additionalProperties": false
            })),
        }
    }

    fn valid_response_body() -> Vec<u8> {
        serde_json::to_vec(&json!({
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "{\"ok\":true}"
                }]
            }],
            "usage": {"input_tokens": 20, "output_tokens": 30}
        }))
        .expect("valid response envelope")
    }

    fn provider_for_test(endpoint: String) -> OpenAiResponsesProvider {
        OpenAiResponsesProvider {
            client: build_http_client(Duration::from_secs(2)).expect("bounded test client"),
            endpoint,
            api_key: TEST_API_KEY.to_owned(),
        }
    }

    fn http_response(status: u16, headers: &str, body: &[u8]) -> String {
        let extra_headers = if headers.is_empty() {
            String::new()
        } else {
            format!("{headers}\r\n")
        };
        format!(
            "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{}",
            body.len(),
            String::from_utf8_lossy(body)
        )
    }

    fn request_is_complete(request: &[u8]) -> bool {
        let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let content_length = headers
            .lines()
            .skip(1)
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        request.len() >= headers_end + 4 + content_length
    }

    fn serve_once(response: String) -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("local HTTP listener");
        let address = listener.local_addr().expect("local address");
        let (sender, receiver) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("provider connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("bounded mock read");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            while !request_is_complete(&request) {
                let count = stream.read(&mut chunk).expect("read provider request");
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
            }
            let _ = sender.send(String::from_utf8_lossy(&request).into_owned());
            let _ = stream.write_all(response.as_bytes());
        });
        (format!("http://{address}/v1/responses"), receiver, server)
    }

    async fn generate_from_response(
        response: String,
        maximum_response_bytes: usize,
    ) -> (
        Result<StructuredGenerationResponse, StructuredModelError>,
        String,
    ) {
        let (endpoint, request, server) = serve_once(response);
        let provider = provider_for_test(endpoint);
        let result = provider
            .generate(&generation_request(), maximum_response_bytes)
            .await;
        let request = request
            .recv_timeout(Duration::from_secs(2))
            .expect("provider sent an HTTP request");
        server.join().expect("mock server completes");
        (result, request)
    }

    #[tokio::test]
    async fn maps_request_and_response_without_leaking_provider_or_secret() {
        let body = valid_response_body();
        let response = http_response(200, "", &body);
        let (result, request) = generate_from_response(response, 16_384).await;
        let mapped = result.expect("valid OpenAI response");

        assert_eq!(mapped.output, json!({"ok": true}));
        assert_eq!(mapped.metadata.input_tokens, Some(20));
        assert_eq!(mapped.metadata.output_tokens, Some(30));

        let (headers, body) = request.split_once("\r\n\r\n").expect("HTTP request body");
        assert!(headers.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("authorization")
                    && value
                        .trim()
                        .eq_ignore_ascii_case(&format!("Bearer {TEST_API_KEY}"))
            })
        }));
        let body: Value = serde_json::from_str(body).expect("OpenAI request JSON");
        let generation_request = generation_request();
        assert_eq!(body["model"], "gpt-5.6-luna");
        assert_eq!(body["text"]["format"]["type"], "json_schema");
        assert_eq!(body["text"]["format"]["strict"], true);
        assert_eq!(
            &body["text"]["format"]["schema"],
            generation_request.schema.value()
        );
        assert_eq!(body["store"], false);
        assert!(body.get("provider").is_none());
        assert!(
            !serde_json::to_string(&body)
                .expect("serialized request body")
                .contains(TEST_API_KEY)
        );
    }

    #[tokio::test]
    async fn rejects_invalid_response_envelope_as_permanent() {
        let response = http_response(200, "", br#"{"output":"wrong shape"}"#);
        let (result, _) = generate_from_response(response, 16_384).await;
        let error = result.expect_err("invalid response envelope");

        assert_eq!(
            error.classification,
            StructuredModelErrorClassification::Permanent
        );
        assert_eq!(error.code(), "provider_envelope_invalid");
    }

    #[test]
    fn maps_non_json_model_text_to_the_schema_retry_sentinel() {
        let response = json!({
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "not-json"}]
            }]
        });
        let mapped =
            parse_openai_response(&serde_json::to_vec(&response).expect("response envelope JSON"))
                .expect("valid outer envelope");

        assert_eq!(mapped.output, Value::Null);
    }

    #[tokio::test]
    async fn classifies_http_client_and_server_errors_for_resilience() {
        for (status, classification) in [
            (400, StructuredModelErrorClassification::Permanent),
            (429, StructuredModelErrorClassification::Transient),
            (503, StructuredModelErrorClassification::Transient),
        ] {
            let response = http_response(status, "", b"");
            let (result, _) = generate_from_response(response, 16_384).await;
            let error = result.expect_err("non-success HTTP status");

            assert_eq!(error.classification, classification);
            assert_eq!(error.code(), format!("provider_http_{status}"));
        }
    }

    #[tokio::test]
    async fn classifies_connection_failures_as_transient_without_raw_error_text() {
        let unused_listener = TcpListener::bind("127.0.0.1:0").expect("unused local port");
        let address = unused_listener.local_addr().expect("unused port address");
        drop(unused_listener);
        let provider = provider_for_test(format!("http://{address}/v1/responses"));
        let error = provider
            .generate(&generation_request(), 16_384)
            .await
            .expect_err("connection failure");

        assert_eq!(
            error.classification,
            StructuredModelErrorClassification::Transient
        );
        assert_eq!(error.code(), "provider_transport_error");
    }

    #[tokio::test]
    async fn rejects_declared_oversized_response_before_reading_body() {
        let response = http_response(200, "", b"0123456789abcdef");
        let (result, _) = generate_from_response(response, 8).await;
        let error = result.expect_err("declared response exceeds bound");

        assert_eq!(
            error.classification,
            StructuredModelErrorClassification::Permanent
        );
        assert_eq!(error.code(), "provider_response_too_large");
    }

    #[tokio::test]
    async fn rejects_streamed_oversized_response_without_content_length() {
        let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\n12345\r\n5\r\n67890\r\n0\r\n\r\n".to_owned();
        let (result, _) = generate_from_response(response, 8).await;
        let error = result.expect_err("streamed response exceeds bound");

        assert_eq!(
            error.classification,
            StructuredModelErrorClassification::Permanent
        );
        assert_eq!(error.code(), "provider_response_too_large");
    }

    #[tokio::test]
    async fn does_not_follow_provider_redirects() {
        let unused_listener = TcpListener::bind("127.0.0.1:0").expect("unused local port");
        let redirect_target = unused_listener.local_addr().expect("unused port address");
        drop(unused_listener);
        let headers = format!("Location: https://{redirect_target}/\r\n");
        let response = http_response(302, &headers, b"");
        let (result, _) = generate_from_response(response, 16_384).await;
        let error = result.expect_err("redirect is returned instead of followed");

        assert_eq!(
            error.classification,
            StructuredModelErrorClassification::Permanent
        );
        assert_eq!(error.code(), "provider_http_302");
    }

    #[test]
    fn validates_https_and_constructs_non_redirecting_client() {
        let valid = config();
        OpenAiResponsesProvider::new(&valid).expect("valid provider configuration");

        let mut invalid = valid;
        invalid.endpoint = "http://provider.example/v1/responses".to_owned();
        assert!(OpenAiResponsesProvider::new(&invalid).is_err());
    }
}
