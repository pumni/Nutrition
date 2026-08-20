# Vietnamese catalog coverage analysis

Status: deterministic evidence report; not curation approval or production evidence

The reproducible analysis is implemented by
`scripts/analyze-vietnamese-catalog-coverage.ps1`. It compares the public
VietnameseMealBench parse phrases with the exact `vi-VN` names and contextual
portion units declared by the test-only foundation seed. It does not query or
mutate PostgreSQL, add aliases, create recipes, infer nutrition, or generate
gram conversions.

Run it with an output path outside the repository:

```powershell
$report = "C:\secure\vmb-catalog-coverage\coverage.json"
.\scripts\analyze-vietnamese-catalog-coverage.ps1 -OutputPath $report
```

The report contains source SHA-256 references, item-level classifications,
aggregate counts, candidate names, and an explicit
`not_approved_by_this_analysis` boundary for every result.

## Snapshot and method

The current snapshot is VietnameseMealBench `foundation-0.5.1`,
`development-only`: 15 public cases and 16 parsed items. The catalog model is
the test-only `seeds/0001_foundation_fixture.sql`, with two exact curated
`vi-VN` names:

- `Trứng gà luộc`, with the test-only `quả` portion;
- `Cơm trắng`, with the test-only `bát` portion.

The classifier applies the same diacritic-preserving, whitespace-collapsed
exact-key rule as the application. It uses a strict token-subset relationship
only to report a **candidate** missing alias; it never treats that relationship
as a runtime match. Composite/recipe tags are reported as recipe evidence
needs, and missing or unsupported mass units add a separate portion-evidence
dimension. Classifications are multi-dimensional; a portion gap can coexist
with a missing identity or recipe gap.

Source hashes for the baseline snapshot:

| Source | SHA-256 |
|---|---|
| `fixtures/vietnamese-meal-bench/manifest.json` | `dc7da868d0be08f4583717f3fb3ed09a74d33b5338ea4cd4ac299a4db05f9ede` |
| `fixtures/vietnamese-meal-bench/public-test-cases.json` | `ad11b9061c4c150383590c43b10be15cda8cfb944f9ce2e295e49bd24cf96f32` |
| `seeds/0001_foundation_fixture.sql` | `457cbe5999e6559dadb70321a166f73f1955558a8b0d4c43f70839081e039e84` |

## Aggregate result

Primary classes sum to the 16 parsed items. The dimension count is multi-label.

| Primary class / dimension | Count |
|---|---:|
| Resolvable exact identity | 3 |
| Missing `vi-VN` alias candidate | 2 |
| Missing identity | 8 |
| Preparation mismatch | 0 |
| Recipe evidence needed | 3 |
| Portion evidence needed (including secondary dimensions) | 12 |
| Intentionally insufficient/unknown parse-rejected case | 1 case |

The three exact items are the two `cơm trắng`/`trứng gà luộc` foundation names
in cases where the requested gram or test-only portion unit is represented.
The test-only foundation seed remains `production_eligible=false`.

## Gap observations

- **Alias candidates, not approved aliases:** `trứng luộc` has the strict
  candidate `Trứng gà luộc`; `cơm` has the strict candidate `Cơm trắng`. Both
  remain owner-review decisions because a broad phrase can be ambiguous.
- **Missing identities:** `thịt bò`, `bơ`, `cơm gà`, `da gà`, `thịt gà`,
  `sữa tươi`, `thịt gà luộc`, and `rau muống` have no exact or deterministic
  candidate identity in the current seed. The analysis does not propose
  aliases or composition values for them.
- **Recipe evidence:** `phở bò`, `bún bò Huế`, and `bánh mì` are composite or
  recipe-dependent benchmark items. A parser success is not composition
  evidence.
- **Preparation mismatch:** no current public item produced this class. The
  classifier retains it for a future case where food identity is known but the
  preparation state is not represented by the catalog evidence.
- **Portion evidence:** 12 items carry a portion dimension, including items
  whose identity or recipe evidence must be resolved first. Unsupported units
  and missing quantity/unit remain clarification or insufficient-evidence
  outcomes; they are not converted by this report.
- **Intentional insufficiency:** the prompt-injection case is parse-rejected
  and remains intentionally insufficient; it is not a catalog gap to curate.

## Smallest owner decisions for the next slice

1. Approve the alias/identity policy for the two candidate phrases, including
   whether either broad phrase is allowed to map to the existing food or must
   remain ambiguous. Do not add either alias until this policy and provenance
   are approved.
2. Select one reviewed basic-food identity slice from the missing-identity
   group and approve its source, preparation semantics, and review lifecycle.
   The runtime must continue exact-match behavior until that evidence is
   staged and activated through the existing gates.
3. If an identity slice is approved, select its first portion targets and run
   the portion-evidence protocol for the observed units. Do not reuse the
   synthetic `quả`/`bát` measurements for other foods.
4. Keep the three recipe items behind a separate recipe/source decision. Do
   not approximate their nutrient values from parser output or a nearest basic
   food.

These are decision inputs, not approvals. The report does not change catalog
records, benchmark answers, production eligibility, or public API behavior.
