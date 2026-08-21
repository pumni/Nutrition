# VietnameseMealBench

Current fixture release: `foundation-0.5.1` (`development-only`). Run the structural
report harness with:

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

The report harness validates IDs, locale, separate parse/analysis decision labels, parse
annotation shape, split counts, and required safety fields. Its output contains only aggregate
counts and SHA-256 hashes of the development/public files. It never loads or prints sealed
answers.

Each case has two decisions:

- `expected_parse_decision`: parser extraction only (`parsed` or `parse_rejected`). It describes
  whether the parser can extract a bounded document and safety signal; it does not require a food
  identity or portion evidence to exist.
- `expected_analysis_decision`: downstream analysis after food identity, preparation, portion, and
  available evidence resolution (`resolve`, `needs_clarification`, or `insufficient`). This is the
  decision used for end-to-end analysis evaluation and is intentionally separate from parser
  scoring. When it is `needs_clarification`, `expected_analysis_clarification_dimension` records
  the downstream evidence dimension; it must not be inferred from the parser annotation.

For example, `1 ly cơm trắng` can be `parsed` at parser level while analysis is
`needs_clarification` because the current portion evidence does not support that vessel. Likewise,
`2 quả trứng luộc` is parseable but requires `food_identity` clarification unless a generic egg
identity is explicitly approved; the benchmark must never infer chicken from `trứng` alone.

The public annotations are `pending_human_review`; they are not a parser release gate yet. Two
independent annotators and a domain adjudicator are required before a case can become approved.
Expected parse structure is separate from provider/model results, and exact nutrition values are
only scored where the composition and portion evidence are suitable.

Initial release metrics follow the benchmark specification: schema-valid rate, mention F1,
known-food top-1/top-3 resolution, unknown detection precision, over-resolution rate, calculation
fixture pass rate, replay pass rate, and slice-level regression checks. Aggregate scores must not
hide regressions in safety, unknown-food, negation, ambiguity, or clarification slices.

The independent annotation workflow, external sealed/challenge protocol, prediction contract,
scoring command, and proposed (not yet approved) thresholds are documented in
`docs/evidence/vietnamese-meal-bench-adjudication.md`.
