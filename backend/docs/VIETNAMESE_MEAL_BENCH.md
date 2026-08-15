# VietnameseMealBench

Current fixture release: `foundation-0.4.0` (`development-only`). Run the structural and
aggregate report harness with:

```powershell
.\scripts\verify-vietnamese-meal-bench.ps1
```

The manifest defines four splits:

- `development`: the five deterministic foundation cases used by current engineering tests;
- `public_test`: 15 broader draft annotations covering household units, decimals/fractions,
  multiple items, modifiers, negation, regional names, slang, typos, ambiguity, unsupported
  portions, and prompt-injection text;
- `sealed_test`: externally controlled; no case file or answer is stored in this repository;
- `challenge`: externally controlled production-derived cases after privacy review.

The report harness validates IDs, locale, decision labels, parse annotation shape, split counts,
and required safety fields. Its output contains only aggregate counts and SHA-256 hashes of the
development/public files. It never loads or prints sealed answers.

The public annotations are `pending_human_review`; they are not a parser release gate yet. Two
independent annotators and a domain adjudicator are required before a case can become approved.
Expected parse structure is separate from provider/model results, and exact nutrition values are
only scored where the composition and portion evidence are suitable.

Initial release metrics follow the benchmark specification: schema-valid rate, mention F1,
known-food top-1/top-3 resolution, unknown detection precision, over-resolution rate, calculation
fixture pass rate, replay pass rate, and slice-level regression checks. Aggregate scores must not
hide regressions in safety, unknown-food, negation, ambiguity, or clarification slices.
