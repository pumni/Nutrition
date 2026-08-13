# 07 — Verification and Evals

## Verification philosophy

The ACL exists to convert "please follow these instructions" into testable contracts.

Model judgement is not accepted as proof.

## Gate order

Cheap gates run first:

```text
G0  ACL files exist
G1  JSON syntax/required fields
G2  source-lock freshness
G3  context budget
G4  profile/source/path integrity
G5  task-packet contract
G6  changed-path scope check
G7  repository formatter/lint/tests
G8  module-specific integration gates
```

## `verify-agent-context.ps1` required modes

### Default mode

```powershell
.\scripts\verify-agent-context.ps1
```

Checks repository ACL integrity.

### Task mode

```powershell
.\scripts\verify-agent-context.ps1 -TaskPacket ".agent/tasks/current.json"
```

Additionally:
- validates task packet;
- resolves named profile;
- checks packet allowed/forbidden path consistency;
- inspects current git changes and verifies scope.

### Self-test mode

```powershell
.\scripts\verify-agent-context.ps1 -SelfTest
```

Runs fixture cases from `.agent/evals/context-layer-cases.json`.

## Required integrity checks

1. Required files exist.
2. Every JSON artifact parses.
3. Manifest exact `schema_version`.
4. Context release exact `agent-context-1.0.0`.
5. Context profile names are unique.
6. Every profile path exists.
7. Every source-register source path exists.
8. Every source-lock hash matches.
9. `AGENTS.md` <= 4096 bytes.
10. Authority/invariant/context files obey budgets.
11. Task template contains empty `decision_points`.
12. No profile has an empty verification gate list.
13. `agent-context-maintenance` forbids `crates/**` and `migrations/**`.
14. Root `AGENTS.md` points to `.agent/manifest.json`.
15. No task packet may list the same path in allowed and forbidden sets.
16. If dependency impact is `none`, changed `Cargo.toml`/`Cargo.lock` is a failure.
17. If database impact is `none`, changed `migrations/**` is a failure.
18. If behavior-version impact is `none`, task cannot include behavior-version files in intended changes.
19. Changed files must match at least one allowed pattern and no forbidden pattern.
20. `decision_points` must be empty.

## Required self-test cases

At least these 12 cases:

1. `valid_context_maintenance_packet` → pass.
2. `missing_context_profile` → fail.
3. `unknown_context_profile` → fail.
4. `non_empty_decision_points` → fail.
5. `allowed_and_forbidden_overlap` → fail.
6. `dependency_change_declared_none` → fail.
7. `migration_change_declared_none` → fail.
8. `changed_file_outside_allowlist` → fail.
9. `forbidden_runtime_file_for_acl_task` → fail.
10. `stale_source_hash` → fail.
11. `oversized_agents_md_fixture` → fail.
12. `profile_references_missing_file` → fail.

Self-tests must not mutate real project files. They use temporary directories/fixture objects.

## Integration into existing verification

Modify `scripts/verify.ps1` to execute ACL verification before expensive Cargo checks:

```powershell
Write-Output "Validating agent context layer..."
& "$PSScriptRoot\verify-agent-context.ps1"
```

If ACL verification fails, foundation verification stops.

## Full acceptance for ACL v1

Required commands:

```powershell
.\scripts\verify-agent-context.ps1 -SelfTest
.\scripts\verify-agent-context.ps1
.\scripts\verify.ps1
```

`verify-postgres.ps1` is not required for ACL v1 because the plan forbids persistence/migration changes.

## No weakening rule

The executor may not make a failing gate pass by:
- deleting the gate;
- excluding a path;
- broadening an allowlist;
- reducing a source lock;
- increasing a context budget;
- changing an expected result;
- downgrading Clippy warnings;
- skipping tests.

Any gate change requires an explicit architect packet.
