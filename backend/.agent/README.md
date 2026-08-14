# Agent Context Layer

This directory is the machine-readable operating model for policy-bounded implementation work.

- `authority/` defines human decision ownership, implementation autonomy, and escalation classifications.
- `invariants/` and `policies/` define protected product, security, architecture, and change-impact constraints.
- `context/` is the progressive router and module index. The router is the only active context-routing authority.
- `generated/` contains repository facts regenerated from declared sources.
- `contracts/` contains the Task Spec, plan, state, report, and evidence schemas.
- `verification/` and `maps/` define risk, scope, source, and canonical gate identities.
- `evals/` contains typed behavioral tasks, graders, and verification fixtures.
- `state/source-lock.json` records SHA-256 freshness for declared source dependencies.

The agent starts with the minimal routed context, investigates the repository, owns a mutable implementation plan, and may choose relevant files and implementation order inside the approved scope. Human-owned protected decisions remain fail-closed. Reports reference canonical gate IDs and evidence references; they do not define executable commands.
