# Source register

This register blocks production ingestion until license and provenance review are complete.

| Source code | Purpose | License status | Production activation | Owner | Next evidence |
|---|---|---|---|---|---|
| `foundation_fixture` | Deterministic code and HTTP tests | Internal test-only data | Prohibited | Engineering | Replace with reviewed source-backed seed |
| `usda_fdc` | Candidate source for basic foods | Pending formal project review | Prohibited | Unassigned data owner | Record release, terms snapshot, fields, and adapter plan |
| `vn_curated_seed` | Vietnamese dishes and portions | Not yet selected | Prohibited | Unassigned domain owner | Approve recipe/measurement protocol and evidence rights |

## Activation rule

A source cannot become active until it has:

- publisher and owner;
- license/terms snapshot and permitted-use review;
- release identifier and artifact checksum;
- schema fingerprint;
- importer version;
- validation and impact report;
- reviewer and rollback target.

The two foods in `FixtureCatalog` are synthetic engineering fixtures. Their nutrient values must not
be represented as a curated production catalog release.

