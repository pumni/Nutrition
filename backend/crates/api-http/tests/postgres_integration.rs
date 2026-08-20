//! Rust-owned HTTP contract checks used by `cargo xtask postgres`.

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::{
    env,
    error::Error,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
use tokio::time::sleep;

const DATABASE_URL: &str = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition";
const BASE_URL: &str = "http://127.0.0.1:18081";
const AUTHORIZATION: &str = "Bearer dev:0198f100-0000-7000-8000-000000000098";

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and PostgreSQL 18"]
async fn http_create_read_replay_and_ownership_contract() -> Result<(), Box<dyn Error>> {
    let database_url = env::var("TEST_DATABASE_URL").unwrap_or_else(|_| DATABASE_URL.to_owned());
    let binary = api_binary();
    let mut api = Command::new(binary)
        .env("APP_ENV", "ci")
        .env("DATABASE_URL", &database_url)
        .env("RUN_MIGRATIONS", "false")
        .env("RUN_FOUNDATION_SEED", "false")
        .env("AUTH_MODE", "development")
        .env("PARSER_MODE", "fixture")
        .env("APP_BIND_ADDR", "127.0.0.1:18081")
        .env("API_METRICS_BIND_ADDR", "127.0.0.1:19091")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let client = Client::builder().timeout(Duration::from_secs(5)).build()?;
    let result = async {
        wait_until_ready(&client).await?;
        let (analysis_id, revision_id, key) =
            assert_create_read_replay_and_ownership(&client).await?;
        assert_correction_and_history(&client, &analysis_id, &revision_id).await?;
        assert_clarification_flow(&client).await?;

        let conflict = client
            .post(format!("{BASE_URL}/v1/nutrition/analyses"))
            .header("Authorization", AUTHORIZATION)
            .header("Idempotency-Key", &key)
            .json(&json!({
                "text": "100 g trứng gà luộc",
                "locale": "vi-VN",
                "mode": "balanced"
            }))
            .send()
            .await?;
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        Ok::<(), Box<dyn Error>>(())
    }
    .await;

    let _ = api.kill();
    let _ = api.wait();
    result
}

async fn assert_create_read_replay_and_ownership(
    client: &Client,
) -> Result<(String, Value, String), Box<dyn Error>> {
    let key = format!("xtask-postgres-create-{}", uuid::Uuid::now_v7());
    let request = json!({
        "text": "2 quả trứng gà luộc, 1 bát cơm trắng",
        "locale": "vi-VN",
        "mode": "balanced"
    });
    let created_response = client
        .post(format!("{BASE_URL}/v1/nutrition/analyses"))
        .header("Authorization", AUTHORIZATION)
        .header("Idempotency-Key", &key)
        .json(&request)
        .send()
        .await?;
    assert_eq!(created_response.status(), StatusCode::OK);
    let created: Value = created_response.json().await?;
    assert_eq!(created["status"], "completed");
    let analysis_id = created["analysis_id"]
        .as_str()
        .ok_or("create response omitted analysis_id")?
        .to_owned();
    let revision_id = created["revision_id"].clone();

    let replayed_response = client
        .post(format!("{BASE_URL}/v1/nutrition/analyses"))
        .header("Authorization", AUTHORIZATION)
        .header("Idempotency-Key", &key)
        .json(&request)
        .send()
        .await?;
    assert_eq!(replayed_response.status(), StatusCode::OK);
    let replayed: Value = replayed_response.json().await?;
    assert_eq!(replayed["analysis_id"], analysis_id);
    assert_eq!(replayed["revision_id"], revision_id);

    let read_response = client
        .get(format!("{BASE_URL}/v1/nutrition/analyses/{analysis_id}"))
        .header("Authorization", AUTHORIZATION)
        .send()
        .await?;
    assert_eq!(read_response.status(), StatusCode::OK);
    let read: Value = read_response.json().await?;
    assert_eq!(read["analysis_id"], analysis_id);
    assert_eq!(read["revision_id"], revision_id);

    let unauthorized = client
        .get(format!("{BASE_URL}/v1/nutrition/analyses/{analysis_id}"))
        .send()
        .await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let foreign = client
        .get(format!("{BASE_URL}/v1/nutrition/analyses/{analysis_id}"))
        .header(
            "Authorization",
            "Bearer dev:0198f100-0000-7000-8000-000000000097",
        )
        .send()
        .await?;
    assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
    Ok((analysis_id, revision_id, key))
}

async fn assert_correction_and_history(
    client: &Client,
    analysis_id: &str,
    revision_id: &Value,
) -> Result<(), Box<dyn Error>> {
    let correction_key = format!("xtask-postgres-correction-{}", uuid::Uuid::now_v7());
    let correction_request = json!({
        "base_revision_id": revision_id,
        "item_corrections": [{"item_index": 0, "quantity": 1, "unit": "quả"}]
    });
    let correction_response = client
        .post(format!(
            "{BASE_URL}/v1/nutrition/analyses/{analysis_id}/corrections"
        ))
        .header("Authorization", AUTHORIZATION)
        .header("Idempotency-Key", &correction_key)
        .json(&correction_request)
        .send()
        .await?;
    assert_eq!(correction_response.status(), StatusCode::OK);
    let corrected: Value = correction_response.json().await?;
    assert_eq!(corrected["revision_number"], 2);
    assert_eq!(corrected["items"][0]["estimated_mass_g"], "50");
    let corrected_revision_id = corrected["revision_id"].clone();

    let replay = client
        .post(format!(
            "{BASE_URL}/v1/nutrition/analyses/{analysis_id}/corrections"
        ))
        .header("Authorization", AUTHORIZATION)
        .header("Idempotency-Key", &correction_key)
        .json(&correction_request)
        .send()
        .await?;
    assert_eq!(replay.status(), StatusCode::OK);
    let replayed: Value = replay.json().await?;
    assert_eq!(replayed["revision_id"], corrected_revision_id);

    let original = client
        .get(format!(
            "{BASE_URL}/v1/nutrition/analyses/{analysis_id}/revisions/1"
        ))
        .header("Authorization", AUTHORIZATION)
        .send()
        .await?;
    assert_eq!(original.status(), StatusCode::OK);
    let original: Value = original.json().await?;
    assert_eq!(original["revision_id"], revision_id.clone());
    assert_eq!(original["revision_number"], 1);
    Ok(())
}

async fn assert_clarification_flow(client: &Client) -> Result<(), Box<dyn Error>> {
    let create = client
        .post(format!("{BASE_URL}/v1/nutrition/analyses"))
        .header("Authorization", AUTHORIZATION)
        .header(
            "Idempotency-Key",
            format!(
                "xtask-postgres-clarification-create-{}",
                uuid::Uuid::now_v7()
            ),
        )
        .json(&json!({
            "text": "1 ly cơm trắng",
            "locale": "vi-VN",
            "mode": "balanced"
        }))
        .send()
        .await?;
    assert_eq!(create.status(), StatusCode::OK);
    let clarification: Value = create.json().await?;
    assert_eq!(clarification["status"], "needs_clarification");
    let analysis_id = clarification["analysis_id"]
        .as_str()
        .ok_or("clarification response omitted analysis_id")?;
    let answer = client
        .post(format!(
            "{BASE_URL}/v1/nutrition/analyses/{analysis_id}/clarifications"
        ))
        .header("Authorization", AUTHORIZATION)
        .header(
            "Idempotency-Key",
            format!(
                "xtask-postgres-clarification-answer-{}",
                uuid::Uuid::now_v7()
            ),
        )
        .json(&json!({
            "expected_revision_id": clarification["revision_id"],
            "question_id": clarification["question"]["id"],
            "option_id": "unit:bát",
            "mass_g": null
        }))
        .send()
        .await?;
    assert_eq!(answer.status(), StatusCode::OK);
    let answered: Value = answer.json().await?;
    assert_eq!(answered["status"], "completed");
    assert_eq!(answered["revision_number"], 2);
    assert_eq!(answered["items"][0]["estimated_mass_g"], "150");
    Ok(())
}

async fn wait_until_ready(client: &Client) -> Result<(), Box<dyn Error>> {
    for _ in 0..40 {
        if let Ok(response) = client.get(format!("{BASE_URL}/health/ready")).send().await
            && response.status() == StatusCode::OK
        {
            return Ok(());
        }
        sleep(Duration::from_millis(250)).await;
    }
    Err("PostgreSQL-backed API did not become ready".into())
}

fn api_binary() -> PathBuf {
    for key in ["CARGO_BIN_EXE_api-http", "CARGO_BIN_EXE_api_http"] {
        if let Ok(path) = env::var(key) {
            return path.into();
        }
    }
    let name = if cfg!(windows) {
        "api-http.exe"
    } else {
        "api-http"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug")
        .join(name)
}
