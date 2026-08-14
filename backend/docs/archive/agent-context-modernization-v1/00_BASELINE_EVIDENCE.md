# M01 — Baseline Evidence

Captured: 2026-08-14 (Asia/Bangkok)

This artifact records the pre-modernization state required by Packet M01. It is
evidence only; it does not change authority semantics, task schemas, verifier
behavior, or runtime product behavior.

## Repository baseline

- Branch: `refactor/agent-context-m01`
- Baseline commit before M01 changes: `df210711a4e21d66d9ab562eaeabcb9b962962f7`
- Repository: `pumni/Nutrition_backend`
- Project behavior release: `foundation-0.6.0`
- Working tree before this artifact: clean

## Current context-layer releases

Values are recorded from `.agent/manifest.json`.

| Release field | Value |
| --- | --- |
| `schema_version` | `1.0.0` |
| `context_release` | `agent-context-1.0.0` |
| `contract_release` | `agent-contract-1.1.0` |
| `verifier_release` | `agent-verifier-2.2.0` |
| `verification_registry_release` | `agent-gates-2.2.0` |
| `runner_release` | `agent-runner-1.0.1` |
| `verification_report_release` | `agent-verification-report-2.0.0` |
| `implementation_report_release` | `agent-implementation-report-1.1.0` |
| `ci_release` | `agent-ci-1.0.0` |

## Baseline verification

Gate identities are owned by `.agent/maps/verification-map.json`. Results below
were captured before the M01 artifact was added.

| Canonical gate ID | Result | Evidence note |
| --- | --- | --- |
| `acl-self-test` | PASS | All 56 ACL self-test cases passed. |
| `agent-runner-self-test` | PASS | All runner self-test cases passed. |
| `agent-ci-policy` | PASS | CI workflow policy verification passed. |
| `acl-integrity` | PASS | Agent context verification passed. |
| `foundation-verify` | PASS | Formatting, Clippy, workspace tests, JSON, sensitive-log scan, and Compose validation passed. |

The foundation run reported three PostgreSQL-dependent tests as ignored because
`TEST_DATABASE_URL`/PostgreSQL 18 was not configured. This is recorded as an
environment limitation, not as evidence that PostgreSQL integration passed.

## Context byte sizes

Measured from the nine files under `.agent/contexts/`. The configured per-file
budget is 16,384 bytes.

| Context file | Bytes |
| --- | ---: |
| `.agent/contexts/foundation.md` | 1,298 |
| `.agent/contexts/domain.md` | 1,261 |
| `.agent/contexts/application.md` | 1,300 |
| `.agent/contexts/parser.md` | 1,439 |
| `.agent/contexts/persistence.md` | 1,482 |
| `.agent/contexts/api.md` | 1,309 |
| `.agent/contexts/worker.md` | 1,313 |
| `.agent/contexts/data-governance.md` | 1,421 |
| `.agent/contexts/verification.md` | 2,852 |
| **Total** | **13,675** |

## Source-register and source-lock coverage

Measured from the current machine-readable artifacts:

- source-register artifacts: **15**;
- declared source references: **98**;
- source-lock entries: **8**;
- source-lock algorithm: **SHA256**;
- source register: `.agent/maps/source-register.json`;
- source lock: `.agent/state/source-lock.json`.

This records the baseline partial-lock state. In particular, the source
register already declares implementation-code sources while the lock contains
only eight entries. Source-lock derivation and freshness behavior are deferred
to Packet M03.

## Representative behavioral task inventory

The following 15 cases are identified for the later behavioral-evaluation
baseline. M01 records the inventory; it does not implement the M11 harness.

| ID | Category | Intent |
| --- | --- | --- |
| `BEH-001` | Context discovery | Domain calculator change |
| `BEH-002` | Context discovery | Hosted parser schema change |
| `BEH-003` | Context discovery | Cross-cutting API/application change |
| `BEH-004` | Root cause | Idempotency normalization |
| `BEH-005` | Root cause | Persistence replay mismatch |
| `BEH-006` | Scope | Legitimate extra file discovered inside scope |
| `BEH-007` | Invariant | Domain network-call temptation |
| `BEH-008` | Invariant | LLM nutrition invention temptation |
| `BEH-009` | Invariant | Sensitive logging temptation |
| `BEH-010` | Protected decision | Unspecified migration/schema change |
| `BEH-011` | Protected decision | Unspecified public API behavior |
| `BEH-012` | Recovery | First implementation fails regression test |
| `BEH-013` | Recovery | Stale initial hypothesis |
| `BEH-014` | Efficiency | Narrow domain task |
| `BEH-015` | Diff review | Accidental unrelated change |

## M01 scope result

- Required baseline evidence: recorded.
- Authority semantics changed: no.
- Task schema changed: no.
- Verifier behavior changed: no.
- Runtime product behavior changed: no.
- Protected decisions encountered: none.
- Deferred work: M02 and later packets, including the M11 behavioral harness.
