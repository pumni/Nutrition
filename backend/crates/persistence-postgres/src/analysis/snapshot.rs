//! Snapshot serialization and hash verification responsibility.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(crate) fn snapshot_hash(snapshot_value: &Value) -> Result<String, ApplicationError> {
    let bytes = serde_json::to_vec(snapshot_value).map_err(|_| ApplicationError::Persistence)?;
    Ok(sha256_hex(&bytes))
}

pub(crate) fn verified_snapshot_value(
    row: &sqlx::postgres::PgRow,
) -> Result<Value, ApplicationError> {
    let value: Value = row
        .try_get("result_snapshot")
        .map_err(|_| ApplicationError::Persistence)?;
    let expected_hash: String = row
        .try_get("snapshot_hash")
        .map_err(|_| ApplicationError::Persistence)?;
    if snapshot_hash(&value)? != expected_hash {
        return Err(ApplicationError::Persistence);
    }
    Ok(value)
}

pub(crate) fn decode_snapshot_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AnalysisSnapshot, ApplicationError> {
    serde_json::from_value(verified_snapshot_value(row)?).map_err(|_| ApplicationError::Persistence)
}

pub(crate) fn sha256_hex(value: &[u8]) -> String {
    encode(Sha256::digest(value))
}

pub(crate) fn overall_quality(snapshot: &AnalysisSnapshot) -> &'static str {
    if snapshot.items.iter().all(|item| {
        matches!(
            item.evidence_quality,
            EvidenceQuality::A | EvidenceQuality::B
        )
    }) {
        "high"
    } else if snapshot
        .items
        .iter()
        .any(|item| item.evidence_quality == EvidenceQuality::U)
    {
        "insufficient"
    } else {
        "medium"
    }
}
