# P00 — Preflight

## Authority

This packet makes no code/design changes. It validates assumptions only.

## Objective

Prove the executor's checkout matches the planned baseline and existing repository is green before ACL files are created.

## Expected baseline

`da04e773a214e8f8232db149d1f35f3f0bd61ce1`

## Allowed paths

None. No writes.

## Steps

1. Run `git rev-parse HEAD`.
2. Compare exact SHA with expected baseline.
3. Run `git status --short`.
4. Check whether `AGENTS.md` exists.
5. Check whether `.agent/` exists.
6. Check whether `scripts/verify-agent-context.ps1` exists.
7. Run current `.\scripts\verify.ps1`.
8. Record results.

## Stop conditions

- SHA differs → `BLOCKED_BASELINE_DRIFT`.
- Unexpected existing ACL artifacts → `BLOCKED_IMPLEMENTATION_MISMATCH`.
- Current verify fails → `BLOCKED_VERIFICATION_FAILURE`.
- Conflicting uncommitted changes → `BLOCKED_SCOPE_CONFLICT`.

## Acceptance

- exact baseline;
- no conflicting ACL;
- current foundation verification passes;
- no files changed.
