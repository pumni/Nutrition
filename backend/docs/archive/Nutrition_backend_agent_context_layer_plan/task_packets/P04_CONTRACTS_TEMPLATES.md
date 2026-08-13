# P04 — Contracts and Templates

## Allowed paths

- `.agent/contracts/**`
- `.agent/templates/**`

## Exact files

Contracts:
- task-packet.schema.json
- verification-report.schema.json
- implementation-report.schema.json

Templates:
- task-packet.example.json
- verification-report.example.json
- implementation-report.example.md

## Hard requirements

- JSON Schema dialect is declared consistently.
- Core objects use `additionalProperties: false`.
- Task packet requires explicit impacts.
- `decision_points` is required and has `maxItems: 0`.
- Example task packet targets ACL maintenance only; no runtime code.
- Example contains no secrets.
- All JSON examples parse.

## Acceptance

All files created and consistent with `06_MACHINE_READABLE_CONTRACTS.md`.
