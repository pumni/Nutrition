use application::ApplicationError;
use domain::{AnalysisId, UserId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const HMAC_BLOCK_SIZE: usize = 64;
const CURSOR_TTL_SECONDS: i64 = 24 * 60 * 60;

#[derive(Clone, Debug)]
pub(crate) struct CursorPosition {
    pub(crate) snapshot_epoch_seconds: i64,
    pub(crate) after_created_at: String,
    pub(crate) after_analysis_id: AnalysisId,
}

#[derive(Deserialize, Serialize)]
struct CursorClaims {
    version: u8,
    principal: UserId,
    status: Option<String>,
    locale: Option<String>,
    snapshot_epoch_seconds: i64,
    after_created_at: String,
    after_analysis_id: AnalysisId,
    expires_at: i64,
}

pub(crate) fn now_epoch_seconds() -> Result<i64, ApplicationError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .map_err(|_| ApplicationError::InvalidCursor)
}

pub(crate) fn encode(
    secret: &[u8],
    principal: UserId,
    status: Option<&str>,
    locale: Option<&str>,
    position: &CursorPosition,
    now: i64,
) -> Result<String, ApplicationError> {
    let claims = CursorClaims {
        version: 1,
        principal,
        status: status.map(str::to_owned),
        locale: locale.map(str::to_owned),
        snapshot_epoch_seconds: position.snapshot_epoch_seconds,
        after_created_at: position.after_created_at.clone(),
        after_analysis_id: position.after_analysis_id,
        expires_at: now
            .checked_add(CURSOR_TTL_SECONDS)
            .ok_or(ApplicationError::InvalidCursor)?,
    };
    let payload = serde_json::to_vec(&claims).map_err(|_| ApplicationError::InvalidCursor)?;
    let signature = hmac_sha256(secret, &payload);
    Ok(format!(
        "v1.{}.{}",
        hex::encode(payload),
        hex::encode(signature)
    ))
}

pub(crate) fn decode(
    secret: &[u8],
    encoded: &str,
    principal: UserId,
    status: Option<&str>,
    locale: Option<&str>,
    now: i64,
) -> Result<CursorPosition, ApplicationError> {
    let parts = encoded.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "v1" {
        return Err(ApplicationError::InvalidCursor);
    }
    let payload_hex = parts[1];
    let signature_hex = parts[2];
    let payload = hex::decode(payload_hex).map_err(|_| ApplicationError::InvalidCursor)?;
    let provided_signature =
        hex::decode(signature_hex).map_err(|_| ApplicationError::InvalidCursor)?;
    let expected_signature = hmac_sha256(secret, &payload);
    if !constant_time_equal(&provided_signature, &expected_signature) {
        return Err(ApplicationError::InvalidCursor);
    }
    let claims: CursorClaims =
        serde_json::from_slice(&payload).map_err(|_| ApplicationError::InvalidCursor)?;
    if claims.version != 1
        || claims.principal != principal
        || claims.status.as_deref() != status
        || claims.locale.as_deref() != locale
        || claims.expires_at <= now
        || claims.snapshot_epoch_seconds <= 0
        || claims.after_created_at.is_empty()
    {
        return Err(ApplicationError::InvalidCursor);
    }
    Ok(CursorPosition {
        snapshot_epoch_seconds: claims.snapshot_epoch_seconds,
        after_created_at: claims.after_created_at,
        after_analysis_id: claims.after_analysis_id,
    })
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut key_block = [0_u8; HMAC_BLOCK_SIZE];
    if key.len() > HMAC_BLOCK_SIZE {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; HMAC_BLOCK_SIZE];
    let mut outer_pad = [0x5c_u8; HMAC_BLOCK_SIZE];
    for index in 0..HMAC_BLOCK_SIZE {
        inner_pad[index] ^= key_block[index];
        outer_pad[index] ^= key_block[index];
    }
    let inner = Sha256::new()
        .chain_update(inner_pad)
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(outer_pad)
        .chain_update(inner)
        .finalize()
        .into()
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[cfg(test)]
mod tests {
    use super::{CursorPosition, decode, encode};
    use domain::{AnalysisId, UserId};

    #[test]
    fn cursor_round_trip_binds_principal_and_filters() {
        let principal = UserId::from_uuid(uuid::Uuid::nil());
        let position = CursorPosition {
            snapshot_epoch_seconds: 1_700_000_000,
            after_created_at: "2026-08-20 00:00:00+00".to_owned(),
            after_analysis_id: AnalysisId::from_uuid(uuid::Uuid::nil()),
        };
        let encoded = encode(
            b"test-secret",
            principal,
            Some("completed"),
            Some("vi-VN"),
            &position,
            1_700_000_001,
        )
        .expect("cursor encodes");
        let decoded = decode(
            b"test-secret",
            &encoded,
            principal,
            Some("completed"),
            Some("vi-VN"),
            1_700_000_001,
        )
        .expect("cursor decodes");
        assert_eq!(
            decoded.snapshot_epoch_seconds,
            position.snapshot_epoch_seconds
        );
        assert_eq!(decoded.after_created_at, position.after_created_at);
        assert!(
            decode(
                b"test-secret",
                &encoded,
                principal,
                Some("needs_clarification"),
                Some("vi-VN"),
                1_700_000_001,
            )
            .is_err()
        );
    }

    #[test]
    fn cursor_rejects_tampering_and_expiry() {
        let principal = UserId::from_uuid(uuid::Uuid::nil());
        let position = CursorPosition {
            snapshot_epoch_seconds: 1_700_000_000,
            after_created_at: "2026-08-20 00:00:00+00".to_owned(),
            after_analysis_id: AnalysisId::from_uuid(uuid::Uuid::nil()),
        };
        let encoded = encode(
            b"test-secret",
            principal,
            None,
            None,
            &position,
            1_700_000_001,
        )
        .expect("cursor encodes");
        let mut tampered = encoded.clone();
        tampered.replace_range(3..4, "0");
        assert!(
            decode(
                b"test-secret",
                &tampered,
                principal,
                None,
                None,
                1_700_000_001
            )
            .is_err()
        );
        assert!(
            decode(
                b"test-secret",
                &encoded,
                principal,
                None,
                None,
                1_700_086_401
            )
            .is_err()
        );
    }
}
