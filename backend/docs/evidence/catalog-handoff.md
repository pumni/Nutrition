# Catalog handoff v1

The Data Factory → Backend boundary uses the canonical shared contract under
[`/contracts/catalog-handoff/v1`](../../../contracts/catalog-handoff/v1/README.md). The initial
profile is `fdc-foundation-reviewed-selection-v1`: USDA FDC Foundation `2026-04-30`, the reviewed
20-record allowlist, exact source identity, and four backend-supported nutrients.

The package is not a deployment or activation artifact. Its manifest is permanently constrained to:

```text
import_mode = staged_only
production_eligible = false
activation_authorized = false
```

The backend resolves the package root safely, validates the embedded canonical JSON Schemas,
checks every declared payload hash and `checksums.sha256`, validates selection/provenance and all
references, then opens one PostgreSQL transaction. Raw source records, exact mappings,
source-backed English names, and in-review composition profiles are staged. No Vietnamese aliases,
recipes, portions, inferred nutrient mappings, or production approval are created.

The worker path is opt-in with `RUN_CATALOG_HANDOFF_IMPORT=true` and is rejected in production.
`CATALOG_HANDOFF_CREATED_BY` is supplied by the backend runtime rather than trusted from package
metadata. Replaying an identical package returns the same release IDs; changing content for the
same logical release returns a typed release conflict. Catalog activation continues through the
existing explicit human-controlled activation API.
