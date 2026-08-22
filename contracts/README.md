# Shared contracts

This directory owns machine-readable contracts shared between `data-factory` and `backend`.
No component may silently define a competing copy of a shared contract.

## Catalog handoff v1

[`catalog-handoff/v1`](catalog-handoff/v1/README.md) is the canonical shared machine contract
for the deterministic, staged-only handoff from Data Factory to the Backend. Its first profile is
`fdc-foundation-reviewed-selection-v1`. The contract binds every payload file to a manifest and a
non-recursive checksum file; consumers must validate the complete package before opening a database
transaction.

The handoff can create only a PostgreSQL `staged` catalog release. Activation is a separate,
human-controlled backend operation.
