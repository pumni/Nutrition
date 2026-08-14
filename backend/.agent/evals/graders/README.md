# Behavioral evaluation graders

`scripts/grade-agent-behavior.ps1` is the canonical grader. It observes the disposable worktree after the adapter exits, checks changed paths against the task scope, runs required gate IDs, and records evidence references. It does not trust the adapter's self-reported success.

The aggregate result is valid only when the harness metadata identifies the adapter/model and every case has persisted trial evidence.
