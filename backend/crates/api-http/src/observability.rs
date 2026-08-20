use axum::http::Method;
use axum::{extract::Request, middleware::Next, response::Response};
use metrics::{counter, histogram};
use sha2::{Digest, Sha256};
use std::time::Instant;

/// Records only normalized route classes. Raw paths are deliberately excluded because they may
/// contain high-cardinality resource identifiers.
pub(crate) async fn observe_http(request: Request, next: Next) -> Response {
    let method = normalized_method(request.method());
    let route = route_class(method, request.uri().path());
    let started = Instant::now();
    let response = next.run(request).await;
    let status = response.status();
    let status_class = status_class(status.as_u16());
    let outcome = outcome(status.as_u16());
    let labels = [
        ("method", method.to_owned()),
        ("route", route.to_owned()),
        ("status_class", status_class.to_owned()),
        ("outcome", outcome.to_owned()),
    ];
    counter!("nutrition_http_requests_total", &labels).increment(1);
    histogram!("nutrition_http_request_duration_seconds", &labels)
        .record(started.elapsed().as_secs_f64());
    response
}

pub(crate) fn normalized_method(method: &Method) -> &'static str {
    if method == Method::GET {
        "GET"
    } else if method == Method::POST {
        "POST"
    } else if method == Method::DELETE {
        "DELETE"
    } else {
        "OTHER"
    }
}

pub(crate) fn route_class(method: &str, path: &str) -> &'static str {
    match path {
        "/health/live" => "health_live",
        "/health/ready" => "health_ready",
        "/v1/nutrition/analyses" if method == "GET" => "analysis_list",
        "/v1/nutrition/analyses" if method == "POST" => "analysis_create",
        "/v1/nutrition/analyses" => "analysis_collection",
        "/v1/nutrition/me" => "privacy_delete",
        "/v1/nutrition/me/export" => "privacy_export",
        _ if path.ends_with("/workflow") => "analysis_workflow",
        _ if path.ends_with("/clarifications") => "analysis_clarification",
        _ if path.ends_with("/corrections") => "analysis_correction",
        _ if path.contains("/revisions/") => "analysis_revision",
        _ if path.starts_with("/v1/nutrition/analyses/") => "analysis_read",
        _ => "unknown",
    }
}

pub(crate) fn safe_request_id(request: &Request) -> String {
    let value = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("missing");
    if value == "missing" {
        return value.to_owned();
    }
    if let Ok(uuid) = uuid::Uuid::parse_str(value) {
        return uuid.to_string();
    }
    let digest = Sha256::digest(value.as_bytes());
    format!("provided-{}", hex::encode(&digest[..8]))
}

fn status_class(status: u16) -> &'static str {
    match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        _ => "5xx",
    }
}

fn outcome(status: u16) -> &'static str {
    match status {
        200..=399 => "success",
        400..=499 => "client_error",
        _ => "server_error",
    }
}

#[cfg(test)]
mod tests {
    use super::{normalized_method, route_class, safe_request_id};
    use axum::{
        body::Body,
        http::{Method, Request},
    };

    #[test]
    fn route_classes_never_include_resource_identifiers() {
        assert_eq!(
            route_class(
                normalized_method(&Method::GET),
                "/v1/nutrition/analyses/0198f100-0000-7000-8000-000000000098",
            ),
            "analysis_read"
        );
        assert_eq!(
            route_class(
                normalized_method(&Method::GET),
                "/v1/nutrition/analyses/0198f100-0000-7000-8000-000000000098/revisions/2",
            ),
            "analysis_revision"
        );
        assert_eq!(
            route_class(
                normalized_method(&Method::GET),
                "/unexpected/free-form/path"
            ),
            "unknown"
        );
        assert_eq!(
            route_class(normalized_method(&Method::POST), "/v1/nutrition/analyses"),
            "analysis_create"
        );
        assert_eq!(
            normalized_method(&Method::from_bytes(b"CUSTOM").unwrap()),
            "OTHER"
        );
    }

    #[test]
    fn custom_request_ids_are_hashed_only_for_log_correlation() {
        let request = Request::builder()
            .header("x-request-id", "client-controlled-sensitive-value")
            .body(Body::empty())
            .expect("request is valid");
        let safe = safe_request_id(&request);
        assert!(safe.starts_with("provided-"));
        assert!(!safe.contains("client-controlled-sensitive-value"));
    }
}
