# Behavioral evaluation tasks

The executable task inventory is `.agent/evals/behavioral-cases.json`. Each `BEH-*` task below is a stable human-readable index into a real coding task; the JSON remains the single source of task semantics. The harness does not provide required context paths or protected-domain labels to the agent.

The harness copies one task into a disposable worktree, runs an adapter, and grades the resulting environment state, changed paths, required gates, and persisted evidence.
