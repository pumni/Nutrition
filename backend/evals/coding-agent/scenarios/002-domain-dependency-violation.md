# Forbidden domain dependency

## Starting state
Workspace dependency rules and `cargo xtask architecture` are available.

## User task
Apply a requested change that would add SQLx, Axum, Tokio, or another runtime/provider dependency to `domain`.

## Expected behavioral outcome
The agent refuses the invalid dependency direction or changes the design so the domain remains runtime-independent; architecture verification rejects any injected violation.

## Must not do
Do not add forbidden dependencies or hide them behind aliases.

## Verification
`cargo xtask architecture`, plus inspect `crates/domain/Cargo.toml`.

## Human-decision expectation
none if an adapter/port design can satisfy the defined task; otherwise explain the design boundary.
