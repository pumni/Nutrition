# Vietnamese catalog evidence package

Status: staging-only candidate contract; not curation approval or production evidence

P1-102 adds a deterministic validator for externally supplied Vietnamese catalog evidence packages.
It does not create catalog facts. A package may carry proposed or separately human-reviewed identity,
alias, recipe, and portion records, but this repository validator never changes their review state.

## Owner boundary

The package must reference
`docs/decisions/vietnamese-catalog.md#adr-initial-vietnamese-catalog-scope`.
That decision permits implementation and staging preparation while requiring exact/reviewed identities,
human-approved aliases, reviewed recipe evidence, and food-specific portion evidence. It does not
make benchmark fixture values production evidence.

Every package and record is forced to `production_eligible: false`. The package is candidate-only and
cannot authorize catalog activation, production traffic, a release tag, or canonical publication.
`OWNER-BE-006` remains the production gate.

## Contract and validator

- Contract: `docs/contracts/vietnamese-catalog-evidence-package-0.1.0.json`
- Validator: `scripts/validate-vietnamese-catalog-evidence.ps1`
- Regression self-test: `scripts/test-vietnamese-catalog-evidence.ps1`

Run the self-test:

```powershell
.\scripts\test-vietnamese-catalog-evidence.ps1
```

Validate an owner-supplied package with an output path outside the repository:

```powershell
.\scripts\validate-vietnamese-catalog-evidence.ps1 `
  -PackagePath C:\secure\catalog-evidence\package.json `
  -OutputPath C:\secure\catalog-evidence\validation-report.json
```

The validator requires SHA-256-bound provenance, safe evidence references, a review state, a draft or
staged release state, array-shaped collections, package/record identifier patterns, and record-specific
fields. It rejects Windows absolute references and the prohibited Vietnam FCT 2017 source. Proposed
records must use `reviewer_role: "none"`; claiming human review requires an explicit human reviewer role
and decision reference in the supplied package. The validator does not authenticate that person or convert
machine output into human review.

## Record-specific safety

- Identity records require a canonical internal food identity and name.
- Alias records require a canonical target and do not enable runtime fuzzy matching.
- Recipe records require exact ingredient IDs, positive quantities, units, and cooked yield/output.
- Portion records require food identity, preparation state, measure context, study/protocol identity,
  independent sample masses, replayable bounds, and `lower <= central <= upper`.

No database, provider, seed, migration, or release activation is touched. A missing or invalid
evidence package fails closed and remains outside production publication.
