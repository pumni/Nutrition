# Unsupported portion must not guess

## Starting state
Portion evidence is contextual and unsupported units require clarification.

## User task
Handle an unsupported portion such as an unqualified bowl, cup, fruit, or serving.

## Expected behavioral outcome
The service asks for clarification or returns bounded evidence-backed mass ranges.

## Must not do
Do not add a hidden global gram conversion for `bát`, `ly`, `quả`, or another unit.

## Verification
Portion fixture tests, application tests, and `cargo xtask check`.

## Human-decision expectation
none unless the requested product policy is undefined.
