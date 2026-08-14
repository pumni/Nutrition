# Behavioral evaluation graders

scripts/grade-agent-behavior.ps1 is the canonical grader. It observes the disposable worktree after the adapter exits, checks real changed paths against the task scope and central protected-path policy, runs required gate IDs, invokes hidden objective assertions from the external evaluator control plane, inspects the final environment and blocker report when appropriate, and records evidence references. It does not trust adapter self-reports or keyword-only notes.

The aggregate result is valid only when the harness metadata identifies the adapter/model, the control plane is outside the subject root, hidden assertions pass, and every case has persisted trial evidence.
