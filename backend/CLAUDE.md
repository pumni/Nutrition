# Claude Code

Read @AGENTS.md before non-trivial work; it is the vendor-neutral repository contract and context
map.

Use `.claude/rules/` only when the files being changed match a rule. Use a project Skill only when
its workflow matches the task; do not preload unrelated procedures.

For broad investigation, use an isolated subagent when that keeps raw exploration out of the main
context. Return only verified paths, facts, risks, and recommended next actions.

When compacting a session, preserve the objective and acceptance criteria, relevant or modified
paths, verified test results, established decisions, unresolved blockers, and next action. Drop
raw exploration and obsolete hypotheses.
