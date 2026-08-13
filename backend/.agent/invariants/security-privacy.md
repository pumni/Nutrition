# Security and Privacy Invariants

- Meal text, source item text, authorization headers, and database URLs are prohibited in logs. HTTP tracing records metadata without bodies or headers.
- Request bodies are capped at 16 KiB.
- Development authentication uses the explicit development bearer format only. Production startup remains unsupported until a reviewed OIDC/OAuth adapter validates issuer, audience, signature, expiry, and role claims.
- Analysis reads and writes enforce authenticated ownership of the immutable analysis user ID; missing credentials return `401` and another owner's resource returns `403`.
- Hosted telemetry excludes raw request/response content, user identity, authorization, meal text, and secrets. Raw meal-text encryption, retention, deletion, and export require approved product/legal policy.
- Suspected credential, meal-history, or catalog exposure requires disabling the affected adapter, preserving audit evidence, rotating credentials, identifying affected analysis IDs, and following the reviewed notification and retention policy.

Sources:

- `docs/FOUNDATION_DECISIONS.md`
- `docs/HOSTED_PARSER.md`
- `docs/SECURITY_AND_OPERATIONS.md`
- `docs/RISK_REGISTER.md`
