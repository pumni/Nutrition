# Verification Context

Derived from: AGENTS.md, docs/AGENT_ENGINEERING.md, scripts/compile-agent-task-spec.ps1, scripts/prepare-agent-task.ps1, scripts/verify-agent-context.ps1, scripts/run-agent-verification.ps1, scripts/verify-agent-behavior.ps1, scripts/grade-agent-behavior.ps1, scripts/run-agent-behavior.ps1, scripts/run-codex-behavior-adapter.ps1, .agent/authority, .agent/contracts, .agent/maps, .agent/verification, .agent/context, .github/workflows/agent-context-integrity.yml, .github/workflows/agent-task-attest.yml.

The trusted prepare phase accepts human Task Intent and a caller-supplied baseline commit captured before agent execution, writes a compiled Task Spec outside the target worktree, and binds the scope ceiling, approvals, and risk floor. The verification runner consumes that immutable spec, requires a distinct target commit for trusted attestation, checks baseline-to-target ancestry, derives context and executable gate IDs from ControlRoot policy against the observed diff, and writes bounded evidence outside the target worktree. Protected paths remain fail-closed. Reports reference gate IDs and evidence identity; they do not define commands.

Behavioral task metadata and hidden assertions belong to the evaluator control plane. A subject worktree receives only the objective, acceptance criteria, and coarse scope needed for the trial.
