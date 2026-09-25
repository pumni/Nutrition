//! `OpenAI` Responses envelope mapping.

#![allow(clippy::wildcard_imports)]

use crate::{
    StructuredGenerationRequest, StructuredGenerationResponse, StructuredModelError,
    StructuredModelErrorClassification, StructuredResponseMetadata,
};
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) fn openai_responses_request(request: &StructuredGenerationRequest) -> Value {
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

pub(crate) fn parse_openai_response(
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
