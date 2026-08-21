---
name: verify
description: Choose the smallest sufficient verification path for a Nutrition_backend change and finish with repository evidence.
---

# Verify

1. Read `AGENTS.md` and identify the changed concern.
2. Run targeted tests first.
3. Run `cargo xtask check` for normal completion. Add `cargo xtask postgres`, `cargo xtask fdc`,
   `cargo xtask containers`, or `cargo xtask benchmark` when the change reaches that boundary.
4. Inspect `git diff --check`, `git status --short`, and the actual diff.

Report commands and results, not a custom attestation format.
