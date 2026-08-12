# P08 — Final Evaluation

## Allowed writes

None, except a report outside repository or a pre-authorized report path if the architect supplies one.

## Checks

1. `git diff --name-only <baseline>...HEAD` or equivalent local diff.
2. Assert no:
   - `crates/**`
   - `migrations/**`
   - `Cargo.toml`
   - `Cargo.lock`
   - `deploy/**`
   - parser schema
3. Run `.\scripts\verify-agent-context.ps1 -SelfTest`.
4. Run `.\scripts\verify-agent-context.ps1`.
5. Run `.\scripts\verify.ps1`.
6. Validate example task packet in task mode.
7. Run one negative fixture showing a `crates/**` change would be rejected for `agent-context-maintenance`.
8. Check `AGENTS.md` byte size.
9. Produce implementation report.

## Acceptance

All positive gates pass, negative gate fails for expected reason, no scope deviation.

No architecture changes are permitted during this phase.
