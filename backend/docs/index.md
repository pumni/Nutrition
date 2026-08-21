# Documentation Index

Use this page as a router. Read only the concern relevant to the current task, then inspect the
source and direct tests.

## Architecture and product behavior

- [Architecture map](../ARCHITECTURE.md) — request flow, crate ownership, and dependency direction
- [Architecture index](architecture/index.md) — subsystem maps and dependency direction
- [Foundation](architecture/foundation.md) — current deterministic vertical slice
- [Hosted parser](architecture/parser.md) — parser trust boundary and provider behavior
- [Public API](product/api-v1.md) — current HTTP contract and known gaps
- [Product invariants](product/invariants.md) — stable evidence-first behavior
- [Product privacy](product/privacy.md) — data ownership and sensitive-data behavior

## Evidence and data

- [Evidence index](evidence/index.md) — evidence classes and versioning
- [Portions](evidence/portions.md) — contextual measurement evidence
- [Nutrition sources](evidence/nutrition-sources.md) — source, provenance, and activation policy
- [Source register](evidence/sources.md) — approved and deferred evidence sources
- [Vietnamese catalog](evidence/vietnamese-catalog/evidence-package.md)
- [Vietnamese meal benchmark](evidence/vietnamese-meal-bench.md) — development-only boundary

## Operations and delivery

- [Operations index](operations/index.md) — operational map
- [Configuration](operations/configuration.md) — environment matrix and fail-closed settings
- [Security](operations/security.md) — authentication and operational boundary
- [Reliability](operations/reliability.md) — local reliability observation harness
- [Risk register](operations/risk-register.md) — active operational and product risks
- [Release gate](operations/staging-release-gate.md) — staging and release evidence requirements
- [Backup/restore drill](operations/backup-restore.md)

## Decisions and releases

- [Decisions index](decisions/index.md) — lasting ADR-style decisions
- [Release index](releases/index.md) — current release pointer and historical evidence snapshots

Historical design archives are not a required starting context. Git history remains the archive;
active documents above are the current repository navigation surface.
