# Data and Replay Invariants

- Source acquisition is staged: acquire, verify, parse, validate, map, report, approve, then activate. Raw source artifacts are immutable and activation has a rollback target.
- Published recipes, composition profiles, nutrient values, released food names, and portion evidence are immutable; a change creates a new version or release.
- Completed analysis revisions are append-only. Each revision retains its interpretation, evidence, result, immutable snapshot, and version context.
- Persistence writes the analysis workflow, snapshot, hash, and outbox event transactionally. A revision is finalized from `building` to `completed` only after the snapshot and hash are supplied.
- Replay is verified by SHA-256 and cannot depend on unrecorded current configuration.
- Persisted revisions record independent application, parser schema, prompt, provider/model, normalization, resolution, portion, composition selection, clarification, correction, calculator, and catalog release versions.

Sources:

- `docs/FOUNDATION_DECISIONS.md`
- `docs/archive/nutrition_backend_blueprint_v1.0/00_README.md`
- `docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
