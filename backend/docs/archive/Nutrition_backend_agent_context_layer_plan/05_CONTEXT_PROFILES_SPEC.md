# 05 — Context Profiles Specification

The implementation must create these profiles exactly.

## `core`

**Use:** docs/tooling tasks with no product behavior impact.

Context:
- authority contract
- decision policy
- architecture invariant
- security/privacy invariant
- crate map
- verification map

Default gates:
- ACL verification
- repository provider-independent verification if executable files/scripts changed

## `domain-calculation`

Paths normally covered:
- `crates/domain/**`
- calculation-related fixtures/docs

Additional context:
- `.agent/invariants/product-domain.md`
- `.agent/invariants/data-replay.md`
- `.agent/contexts/domain.md`
- blueprint calculation spec
- relevant ADRs 006, 008, 009, 019, 020

Required gates:
- cargo fmt
- clippy workspace
- workspace tests
- calculator/golden tests named by packet
- behavior-version check when semantics change

Forbidden by default:
- network/database/provider dependencies in domain
- rounding intermediate values
- silent change to calculation version

## `application-analysis`

Paths:
- `crates/application/**`

Additional context:
- `.agent/contexts/application.md`
- product/domain invariant
- data/replay invariant
- clarification/correction source spec as required

Required gates:
- cargo fmt
- clippy
- workspace tests
- workflow/state tests named by packet

High-risk:
- parser/evidence/calculation orchestration order
- one-question clarification behavior
- append-only revisions
- behavior version vector

## `parser-hosted`

Paths:
- `crates/adapters/src/hosted_parser.rs`
- parser schema files
- parser-specific telemetry integration when explicitly included

Additional context:
- `.agent/invariants/llm-boundary.md`
- `.agent/invariants/security-privacy.md`
- `.agent/contexts/parser.md`
- `docs/HOSTED_PARSER.md`
- parser/security risks
- ADR-010 and ADR-016

Required gates:
- cargo fmt
- clippy
- workspace tests
- hosted parser unit/integration tests
- schema validation
- adversarial benchmark gate if behavior/prompt/schema/provider changes

Forbidden:
- nutrition values from model
- IDs/gram estimates from model
- provider tools
- raw text telemetry
- redirect forwarding
- silent fixture fallback

## `persistence-postgres`

Paths:
- `crates/persistence-postgres/**`
- `migrations/**`

Additional context:
- `.agent/contexts/persistence.md`
- data/replay invariant
- architecture invariant
- database blueprint
- foundation decision immutability/idempotency sections

Required gates:
- cargo fmt
- clippy
- workspace tests
- `scripts/verify-postgres.ps1` for DB/migration changes

Forbidden:
- edit applied migration
- weaken published/revision immutability
- hold DB transaction across LLM/network call
- unversioned replay dependency

## `api-http`

Paths:
- `crates/api-http/**`

Additional context:
- `.agent/contexts/api.md`
- security/privacy invariant
- application context

Required gates:
- cargo fmt
- clippy
- workspace tests
- API smoke/contract tests when endpoint behavior changes
- PostgreSQL verification when ownership/persistence route behavior is touched

High-risk:
- auth
- ownership
- request limits
- idempotency
- internal error leakage
- sensitive logging

## `worker-ops`

Paths:
- `crates/worker/**`
- ops persistence when explicitly included

Additional context:
- `.agent/contexts/worker.md`
- persistence context if jobs/outbox touched
- security/privacy invariant

Required gates:
- cargo fmt
- clippy
- workspace tests
- worker mode/lease/outbox integration tests
- PostgreSQL verification if DB-backed job semantics change

High-risk:
- lease correctness
- retries/dead state
- idempotent effects
- graceful shutdown

## `data-governance`

Paths:
- `seeds/**`
- `fixtures/**`
- source/governance docs
- catalog release artifacts explicitly named in packet

Additional context:
- `.agent/contexts/data-governance.md`
- data/replay invariant
- source register/risk docs
- ADR-014/018

High-risk:
- fixture mistaken for production
- source/license provenance
- automated canonical publication
- release activation

## `agent-context-maintenance`

Paths:
- `AGENTS.md`
- `.agent/**`
- `scripts/verify-agent-context.ps1`
- the exact integration lines in `scripts/verify.ps1`
- ACL README documentation section

Additional context:
- `.agent/README.md`
- all authority files
- `.agent/contexts/verification.md`

Forbidden:
- any file under `crates/**`
- any migration
- Cargo dependency changes
- runtime config changes
- behavior-version changes

Required gates:
- ACL self-tests
- ACL repository validation
- existing `scripts/verify.ps1`
