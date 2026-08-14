# Verification Context

Derived from: AGENTS.md, docs/AGENT_ENGINEERING.md, scripts/compile-agent-task-spec.ps1, scripts/verify-agent-context.ps1, scripts/run-agent-verification.ps1, scripts/verify-agent-behavior.ps1, scripts/grade-agent-behavior.ps1, scripts/run-agent-behavior.ps1, scripts/run-codex-behavior-adapter.ps1, .agent/authority, .agent/contracts, .agent/maps, .agent/verification, .agent/context, .agent/evals/ci-cases.json, .agent/evals/implementation-report-cases.json, .agent/evals/tasks/README.md, .agent/evals/graders/README.md, .agent/templates, .github/workflows/agent-context-integrity.yml, .github/workflows/agent-task-attest.yml.

The trusted runner accepts human Task Intent, binds the current baseline and scope ceiling, validates the compiled Task Spec, derives context and executable gate IDs from actual changed paths, and writes bounded evidence outside the target worktree. Protected paths remain fail-closed. Reports reference gate IDs and evidence identity; they do not define commands.

Behavioral task metadata and hidden assertions belong to the evaluator control plane. A subject worktree receives only the objective, acceptance criteria, and coarse scope needed for the trial.
