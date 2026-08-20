# Coding-agent outcome evals

These evaluator-side scenarios test whether an agent can make defined changes autonomously while preserving the repository's safety and product invariants. They are evaluator control-plane inputs, not subject-repository context, and do not contain sealed benchmark evidence.

Each scenario states the starting state, user task, expected outcome, forbidden outcome, verification evidence, and whether a human decision is required. Grade repository state and command output, not a custom report format.

Run the relevant repository checks for the scenario. The normal completion gate is `cargo xtask check`; use `cargo xtask postgres`, `cargo xtask fdc`, or `cargo xtask benchmark` when the scenario requires those capabilities.

The suite is designed for comparative runs against a pinned baseline and a pinned refactored subject with the same prompt. The subject checkout must be created without \`evals/coding-agent/**\`; the runner owns the scenarios outside the subject context and must start every run from a clean subject snapshot. See [control/protocol.json](control/protocol.json) for the frozen matrix, isolation rule, canonical suite hash, and gates.

The current execution environment has Claude Code 2.1.220 and Codex CLI 0.148.0 installed, but neither CLI is authenticated, so the required comparative evaluation is explicitly blocked. See [results-v2.json](results-v2.json). That artifact contains the pinned SHAs and suite hash, no fabricated metrics, and no P06 pass claim. A future run must replace the blocked record with results from both named agents on the same isolated scenarios.

The subject SHA remains \`c7bc72844a0b8a14733550b9dd630642f8bef547\` for an eval-only/docs-only update. If runtime, context, or harness behavior changes, the subject SHA must change and the full matrix must be rerun.
