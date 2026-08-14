# Agent evaluation suite

The behavioral suite is defined by evaluator-control-plane metadata supplied through an external CasesPath. Case banks, expected paths, forbidden paths, required gates, and hidden assertions must not be present in the subject worktree. A trial runs through a pluggable adapter in a disposable worktree. The grader observes real changed paths, gate outcomes, protected-decision handling, tests, final environment state, and persisted evidence; adapter self-reports are not sufficient for success.

Use scripts/verify-agent-behavior.ps1 -SelfTest -CasesPath <control-plane-case-bank> to validate the external task inventory and result schema. It deliberately does not manufacture behavioral results. Real trials publish adapter/model metadata, a baseline commit, per-case worktree evidence, gate logs, changed paths, hidden assertion results, and environment outcomes outside the repository.
