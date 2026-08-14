# Behavioral evaluation graders

`scripts/grade-agent-behavior.ps1` is the canonical grader. It observes the disposable worktree after the adapter exits, checks real changed paths against the task scope and central protected-path policy, runs required gate IDs, inspects the final environment and blocker report when appropriate, and records evidence references. It does not trust the adapter's self-reported success or keyword-only notes.

The aggregate result is valid only when the harness metadata identifies the adapter/model and every case has persisted trial evidence.
