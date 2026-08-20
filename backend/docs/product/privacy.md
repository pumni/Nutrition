# Product privacy

Privacy behavior is ownership-scoped and fail-closed. Raw meal text, authorization material,
provider secrets, database credentials, and raw hosted-provider responses must not enter logs or
telemetry. Export and deletion operate only on the authenticated owner’s data.

Current operational controls are documented in [security](../operations/security.md) and
[observability](../operations/observability.md). Runtime privacy behavior is covered by the
PostgreSQL privacy integration tests and `cargo xtask privacy`.
