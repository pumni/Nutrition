# P03 — Context Packs, Maps and Profiles

## Objective

Create concise task-selective context.

## Allowed paths

- `.agent/contexts/**`
- `.agent/maps/**`
- `.agent/profiles/**`

## Exact context files

- foundation.md
- domain.md
- application.md
- parser.md
- persistence.md
- api.md
- worker.md
- data-governance.md
- verification.md

## Exact map files

- crate-map.json
- change-impact-map.json
- verification-map.json
- source-register.json

## Exact profile file

- context-profiles.json

## Repository facts to encode

Use the current crate/file structure documented in this plan and inspect the actual checkout to confirm paths.

Do not invent missing modules.

## Profile decisions

Implement exactly the nine profiles in `05_CONTEXT_PROFILES_SPEC.md`.

## Acceptance

- every referenced context/source/path exists;
- no context file exceeds 16 KiB;
- no profile has empty gates;
- `agent-context-maintenance` forbids `crates/**` and `migrations/**`.
