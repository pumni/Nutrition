# Operations index

- [Configuration](configuration.md) — explicit environment and fail-closed settings
- [Security](security.md) — authentication, sensitive data, worker, and incident boundaries
- [Observability](observability.md) — privacy-safe metrics/logging and staging scenarios
- [Reliability](reliability.md) — local reliability observation harness
- [Risk register](risk-register.md) — active risks and owner boundaries
- [Backup/restore](backup-restore.md) — isolated recovery and rollback drill
- [Staging release gate](staging-release-gate.md) — evidence required before release

Normal verification starts with `cargo xtask check`; use `cargo xtask postgres`, `fdc`,
`containers`, or `benchmark` for the relevant operational boundary.
