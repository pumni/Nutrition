# Agent evaluation suite

The behavioral suite is defined by outcome-based coding tasks in `behavioral-cases.json`. A trial runs through a pluggable adapter in a disposable worktree. The grader observes real changed paths, required gate outcomes, protected-decision handling, tests, final environment state, and persisted evidence; adapter self-reports are not sufficient for success.

`scripts/verify-agent-behavior.ps1 -SelfTest` validates the task inventory and result schema only. It deliberately does not manufacture behavioral results. Real trials are run manually, nightly, or at release/harness-change time and publish adapter/model metadata, a baseline commit, per-case worktree evidence, gate logs, changed paths, and environment outcomes outside the repository.
