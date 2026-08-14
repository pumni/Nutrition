# Product and Domain Invariants

- The system produces evidence-based estimates, not a chatbot guess of calories.
- Food identity, portion evidence, composition evidence, source release, and calculation behavior must support reproducibility.
- The LLM understands meal language only; it does not create nutrition facts, canonical food IDs, gram weights, or calories.
- Food and portion resolution remain separate. Unsupported or insufficient evidence is surfaced; household conversions are contextual to food, unit, context, source, region, and quality.
- Estimates expose assumptions, evidence quality, resolution status, and bounded uncertainty. They do not expose an uncalibrated user-facing probability.
- Unknown food is not force-matched into canonical evidence.
- Published data is versioned and immutable. Corrections create a new analysis revision rather than overwriting a completed revision.

Sources:

- `docs/FOUNDATION_DECISIONS.md`
- `docs/RISK_REGISTER.md`
- `docs/FOUNDATION_DECISIONS.md`
