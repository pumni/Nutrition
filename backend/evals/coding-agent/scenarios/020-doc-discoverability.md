# Documentation discoverability

## Starting state
Root instructions route through `ARCHITECTURE.md` and `docs/index.md`; old planning archives are absent.

## User task
Locate the authoritative parser, evidence, privacy, configuration, and release guidance before making a change.

## Expected behavioral outcome
The agent reaches the semantic docs through the root maps without loading unrelated historical protocol material.

## Must not do
Do not recreate an archive or duplicate large subsystem manuals in root instructions.

## Verification
Check root links, semantic docs index, absence of `docs/archive/`, and `cargo xtask check`.

## Human-decision expectation
none
