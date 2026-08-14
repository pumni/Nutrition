# Agent Context Layer

This directory is the machine-readable operating model for policy-bounded implementation work.

- `authority/` defines human decision ownership, implementation autonomy, and escalation classifications.
- `invariants/` define protected product, security, and architecture constraints.
- `context/` is the progressive router and module index. The router is the only active context-routing authority.
- `contracts/` contains Task Intent, compiled Task Spec, optional durable plan/state, report, and evidence schemas.
- `verification/` and `maps/` define risk, scope, provenance, and canonical gate identities.
- `evals/` contains typed behavioral tasks, graders, and verification fixtures.

The agent starts with the minimal routed context, investigates the repository, owns a mutable implementation plan, and may choose relevant files and implementation order inside the approved scope. Human-owned protected decisions remain fail-closed. Reports reference canonical gate IDs and evidence references; they do not define executable commands.
