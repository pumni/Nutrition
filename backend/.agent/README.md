# Agent Context Layer

The `.agent/` tree is compiled repository context for implementation executors. It is repository governance, not runtime product code.

## Directories

- `authority/` contains executor, decision, and escalation rules.
- `invariants/` contains compact non-negotiable product, architecture, data, parser, and security truths.
- `contexts/` contains task-selected context packs.
- `maps/` indexes canonical sources, paths, impacts, and verification gates.
- `profiles/` defines the context files and verification gates for each task profile.
- `contracts/` contains machine-readable task and report contracts.
- `templates/` contains report and task templates.
- `evals/` contains context-layer evaluation cases.
- `state/` contains source-lock state for drift detection.

## Canonical truth

Canonical decisions remain in the repository's existing source documents, ADRs, foundation decisions, implementation, and architect-authored task packets. `.agent/` files are concise compiled context and must not silently redefine those sources. Derived context is indexed through `maps/source-register.json` when the corresponding artifacts are created.

## Profile and task lifecycle

An architect selects the context profile and authors the task packet. The executor reads the required profile context and packet, implements only the packet's allowed paths and sequence, and declares exact `create_files`, `modify_files`, and `delete_files`. `allowed_paths` is only the outer boundary; the deterministic verifier requires exact declared/actual change-type equality, and deletion requires `delete_files`. Gate IDs come from the canonical registry; task packets do not redefine gate commands. The trusted runner resolves execution from ControlRoot, accepts task packets and evidence only from outside TargetRoot, and writes bounded machine evidence outside TargetRoot. The deterministic verifier checks context integrity and scope. Changes to canonical sources require the architect-approved context summaries and source lock to be refreshed.

P10C adds independent GitHub attestation: a default-branch-only dispatch checks separate trusted control and target commits, runs the trusted ControlRoot runner, stores packet/report/attestation evidence under the runner temporary directory, and publishes only the stable `agent-task/verified` status from a status-only job. The canonical CI policy and `agent-ci-policy` gate fail closed on workflow trust-boundary drift.

This layer has no runtime integration and does not add a production dependency.
