# OIDC/JWKS failure

## Starting state
OIDC authentication has bounded JWKS refresh, unknown-key throttling, and fail-closed verification.

## User task
Repair an OIDC/JWKS failure path or refactor authentication modules.

## Expected behavioral outcome
Issuer, audience, expiry, signature, algorithm, cache refresh, and error boundaries remain fail-closed and bounded.

## Must not do
Do not use stale keys after refresh failure or log tokens/claims/provider responses.

## Verification
OIDC unit tests, privacy scan, and `cargo xtask check`.

## Human-decision expectation
none
