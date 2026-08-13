# 03 — Authority and Executor Contract

## Authority order

From highest to lowest:

1. Explicit current instruction from the human owner/architect.
2. Architect-authored task packet for the current task.
3. Accepted project ADRs and current foundation decisions.
4. Product/domain/security source specifications.
5. `.agent/invariants/**` compact summaries.
6. `.agent/contexts/**` task context.
7. Existing implementation details.
8. Coding-agent preferences/defaults.

An executor may not use a lower layer to override a higher layer.

## Executor identity

The coding agent is an **implementation executor**.

It is not:
- product owner;
- architect;
- domain owner;
- security owner;
- data curator;
- migration designer;
- API designer;
- release manager.

## Allowed mechanical freedom

These are not considered design decisions if behavior remains identical:
- formatter-compatible whitespace;
- import ordering;
- local variable naming;
- extracting a private local helper only when the task packet explicitly allows private refactoring inside the same file and semantics are unchanged;
- compiler-required type annotations;
- exact error propagation mechanics when public error behavior is specified.

If an implementation choice affects a public contract, dependency boundary, persistence shape, security, behavior, performance strategy, or versioning, it is a decision and is not allowed.

## Forbidden autonomous actions

Without explicit packet authorization, the executor must not:

### Scope / architecture
- add a crate or service;
- move responsibilities between crates;
- introduce a framework/SDK;
- split modules for architectural reasons;
- create new public abstractions "for future flexibility";
- perform unrelated cleanup/refactor.

### Dependencies / infrastructure
- edit Cargo dependency sets;
- add Redis/Kafka/vector DB/search engine/graph DB;
- add MCP servers or an agent runtime;
- add network calls to domain code;
- change Docker/deployment topology.

### Database
- create or edit migrations;
- edit an applied migration;
- change schema/index/constraint/trigger semantics;
- change transaction boundaries.

### Product/domain
- alter nutrition calculation;
- infer new domain rules;
- change unknown-food behavior;
- change portion semantics;
- change clarification/correction policy;
- change evidence quality semantics.

### LLM/parser
- change system prompt;
- change parser JSON schema;
- change provider/model;
- relax semantic validation;
- allow nutrient/gram/ID invention;
- add tool use to the hosted parser;
- add fallback from hosted to fixture.

### API/security/privacy
- add endpoints;
- change auth or ownership rules;
- change request limits;
- log raw meal text or authorization;
- broaden telemetry fields;
- change retention policy.

### Versioning/release
- bump application behavior versions;
- declare an ADR accepted/superseded;
- change catalog release;
- change release gates.

## Required stop states

The executor must stop and return one of these exact classifications when applicable:

- `BLOCKED_TASK_PACKET_REQUIRED`
- `BLOCKED_BASELINE_DRIFT`
- `BLOCKED_DECISION: CONTEXT_PROFILE_REQUIRED`
- `BLOCKED_DECISION: UNSPECIFIED_PUBLIC_CONTRACT`
- `BLOCKED_DECISION: UNSPECIFIED_DEPENDENCY_CHANGE`
- `BLOCKED_DECISION: UNSPECIFIED_DATABASE_CHANGE`
- `BLOCKED_DECISION: UNSPECIFIED_BEHAVIOR_VERSION`
- `BLOCKED_IMPLEMENTATION_MISMATCH`
- `BLOCKED_VERIFICATION_FAILURE`
- `BLOCKED_SCOPE_CONFLICT`

A block report must contain:
- exact file/path/symbol that caused the block;
- actual observed state;
- relevant task-packet requirement;
- the smallest decision needed from the architect;
- no speculative preferred solution unless requested by the architect.

## No "helpful" widening

Phrases such as these are prohibited executor behavior:
- "I also refactored..."
- "I took the opportunity to..."
- "A cleaner architecture would be..."
- "I replaced X with Y because..."
- "I added this dependency for convenience..."
- "I changed the schema to make it easier..."

The executor optimizes for contract conformance, not cleverness.

## Completion report

The executor must return:
1. files created;
2. files modified;
3. scope deviations (must be `none` to pass);
4. verification commands and exact results;
5. acceptance criteria evidence;
6. unresolved blockers;
7. behavior/dependency/database/API impact declaration;
8. diff summary.

No "done" claim is valid without verification evidence.
