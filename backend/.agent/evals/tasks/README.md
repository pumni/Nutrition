# Behavioral evaluation tasks

The executable task inventory is `.agent/evals/behavioral-cases.json`. Each `BEH-*` task below is a stable human-readable index into the typed `task` object in that inventory; the JSON remains the single source of task semantics.

The harness copies one task into a disposable worktree, runs an adapter, and grades the resulting environment state, changed paths, required gates, and persisted evidence.
