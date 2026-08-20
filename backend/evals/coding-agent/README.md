# Coding-agent outcome evals

These evaluator-side scenarios test whether an agent can make defined changes autonomously while preserving the repository's safety and product invariants. They are intentionally separate from normal agent context and do not contain sealed benchmark evidence.

Each scenario states the starting state, user task, expected outcome, forbidden outcome, verification evidence, and whether a human decision is required. Grade repository state and command output, not a custom report format.

Run the relevant repository checks for the scenario. The normal completion gate is `cargo xtask check`; use `cargo xtask postgres`, `cargo xtask fdc`, or `cargo xtask benchmark` when the scenario requires those capabilities.

The suite is designed for comparative runs against a baseline and the refactored repository with the same prompt. Record pass/fail, regressions, unnecessary escalations, missed decision boundaries, relevant-file discovery, tool calls, context/tokens, and elapsed time when the runner exposes them.

The current execution environment has no Claude Code or external Codex runner, so the required comparative evaluation is explicitly blocked. See [results-v2.json](results-v2.json). That artifact contains no fabricated metrics and does not claim that the refactored harness passes P06. A future run must replace the blocked record with results from both named agents on the same scenarios.
