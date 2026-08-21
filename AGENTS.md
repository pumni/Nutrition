# Nutrition Monorepo — Coding Agent Guide

This repository contains two bounded contexts and a reserved shared-contracts namespace:

/backend
Rust application/backend bounded context.

/data-factory
Offline deterministic evidence producer.

/contracts
Shared versioned machine contracts.

## Stable boundaries

1. Never make data-factory connect directly to backend PostgreSQL.
2. Never place production credentials in data-factory.
3. Never let data-factory activate catalog data.
4. Contract changes require validation on both producer and consumer.
5. Historical evidence is immutable unless correcting a documented factual error through a new superseding record.
6. Run commands from each component directory unless a root command explicitly says otherwise.
7. Do not create a root Cargo workspace.
8. Do not turn the Python project into a Rust workspace member.
9. Do not refactor component architecture during repository migration.
10. Any cross-boundary data transfer must eventually go through /contracts.

## Evidence and privacy

- Never invent nutrition facts, canonical food IDs, gram weights, or calories from model output.
- Unknown or unsupported evidence fails closed.
- Published catalog evidence and completed analysis revisions are immutable and versioned.
- Replay must not depend on unrecorded current configuration.
- Never log raw meal text, authorization material, database credentials, provider secrets, or raw provider responses.
- Development fixtures and seeds remain isolated to local/CI behavior.
- Production catalog activation, provider enablement, deployment, and release publication are human-controlled effects.

## Verification

The normal verification entry point is the root GitHub Actions workflow. Component commands remain independent:

    cd backend
    cargo xtask check
    cargo xtask postgres
    cargo xtask fdc
    cargo xtask containers

    cd ../data-factory
    python scripts/test.py
    python -m compileall -q src tests scripts

Inspect the relevant component source, direct tests, and documentation before editing. Preserve runtime behavior and keep migration, contract, product, and data changes in separate reviewable commits.
