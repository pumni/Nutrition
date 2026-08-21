# Parser schema change

## Starting state
Parser JSON schema, mapping, semantic validation, and behavior versions are explicit.

## User task
Change the parsed-meal schema in a defined way.

## Expected behavioral outcome
Schema, strict mapping, validation, tests, and relevant persisted behavior-version implications are updated consistently.

## Must not do
Do not accept extra model content, bypass semantic validation, or silently change replay semantics.

## Verification
Schema/artifact checks, adapter tests, behavior-version tests, and `cargo xtask check`.

## Human-decision expectation
none if the new field semantics are defined.
