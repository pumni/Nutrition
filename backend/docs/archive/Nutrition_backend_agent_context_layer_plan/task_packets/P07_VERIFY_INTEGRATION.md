# P07 — Integrate ACL with Existing Verification and README

## Allowed modifies

- `scripts/verify.ps1`
- `README.md`

## Forbidden

Everything else in this packet.

## `scripts/verify.ps1` exact change

After strict error preference initialization and before Cargo checks, insert:

```powershell
Write-Output "Validating agent context layer..."
& "$PSScriptRoot\verify-agent-context.ps1"
```

Do not delete, weaken, reorder away, or conditionally skip existing checks beyond placing this cheap gate before them.

## README exact change

Append `## AI coding context layer` section.

Include:
- `AGENTS.md` is entrypoint;
- architect authors task packets;
- coding agent is implementation-only;
- ACL is not nutrition runtime;
- three verification commands;
- point to `.agent/templates/task-packet.example.json`.

Do not rewrite the existing README.

## Acceptance

Run:
- ACL self-test;
- ACL default;
- full `.\scripts\verify.ps1`.

All pass.
