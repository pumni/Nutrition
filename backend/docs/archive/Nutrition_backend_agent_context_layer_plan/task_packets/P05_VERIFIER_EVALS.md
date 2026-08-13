# P05 — Verifier, Evals and Source Lock

## Allowed paths

- `scripts/verify-agent-context.ps1`
- `.agent/evals/**`
- `.agent/state/source-lock.json`

## Source lock exact input list

Compute SHA-256 for exactly:

1. `Cargo.toml`
2. `docs/FOUNDATION_DECISIONS.md`
3. `docs/HOSTED_PARSER.md`
4. `docs/RISK_REGISTER.md`
5. `docs/SECURITY_AND_OPERATIONS.md`
6. `nutrition_backend_blueprint_v1.0/00_README.md`
7. `nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
8. `nutrition_backend_blueprint_v1.0/13_IMPLEMENTATION_CHECKLIST.md`

## Verifier implementation

Implement exact checks/modes in `07_VERIFICATION_AND_EVALS.md` and `09_FILE_BY_FILE_CHANGESET.md`.

No network calls. No installed modules. No repository mutation.

## Self-tests

Implement at least the 12 named cases.

Tests must use temporary files/directories and clean them in `finally`.

## Acceptance

Run:
- `.\scripts\verify-agent-context.ps1 -SelfTest`
- `.\scripts\verify-agent-context.ps1`

Both pass.
