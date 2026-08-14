# Agent evaluation suite

The behavioral suite is defined by typed tasks in `behavioral-cases.json` and indexed under `tasks/`. A trial runs through a pluggable adapter in a disposable worktree. The grader observes the resulting environment, changed paths, required gate outcomes, protected-decision handling, and persisted evidence; adapter self-reports are not sufficient for success.

`scripts/verify-agent-behavior.ps1 -SelfTest` validates the task inventory and result schema only. It deliberately does not manufacture behavioral results. Real baseline and modern trials are persisted under the result directory with adapter/model metadata, baseline and subject commits, per-case evidence, gate logs, and comparison data.
