# Agent Context Layer

The `.agent/` tree is compiled repository context for policy-bounded implementation agents. It is repository governance, not runtime product code.

## Directories

- `authority/` contains executor, decision, and escalation rules.
- `invariants/` contains compact non-negotiable product, architecture, data, parser, and security truths.
- `contexts/` contains task-selected context packs.
- `policies/` contains human-authored boundary and impact policy.
- `generated/` contains deterministic repository facts with provenance and freshness evidence.
- `maps/` indexes canonical sources, paths, and verification gates.
- `profiles/` retains transitional profile aliases; `context/` defines progressive presets and routing.
- `contracts/` contains machine-readable modern task/report contracts plus transitional compatibility contracts.
- `templates/` contains modern report/task templates; `legacy-task-packet.example.json` is compatibility-only.
- `evals/` contains context-layer evaluation cases.
- `state/` contains source-lock state for drift detection.

## Canonical truth

Canonical decisions remain in the repository's existing source documents, ADRs, foundation decisions, implementation, and architect-authored task packets. `.agent/` files are concise compiled context and must not silently redefine those sources. Derived context is indexed through `maps/source-register.json` when the corresponding artifacts are created.

## Profile and task lifecycle

An architect authors the modern Task Spec. The agent starts from the minimal relevant context preset, investigates the repository, owns a mutable implementation plan, and may choose relevant files and implementation sequence inside the approved policy/scope. Modern scope verification checks actual changes against the scope envelope and protected-path approvals. The explicitly named legacy task packet remains available only for compatibility and retains its exact change checks. Gate IDs come from the canonical registry; task artifacts do not redefine gate commands. The trusted runner resolves execution from ControlRoot, accepts task artifacts and evidence only from outside TargetRoot, and writes bounded machine evidence outside TargetRoot. Changes to canonical sources require the architect-approved context summaries and source lock to be refreshed.

P10C adds independent GitHub attestation: a default-branch-only dispatch checks separate trusted control and target commits, runs the trusted ControlRoot runner, stores packet/report/attestation evidence under the runner temporary directory, and publishes only the stable `agent-task/verified` status from a status-only job. The canonical CI policy and `agent-ci-policy` gate fail closed on workflow trust-boundary drift.

This layer has no runtime integration and does not add a production dependency.
