# Vietnamese portion measurement handoff

Current status: `study_design_incomplete_publication_blocked`.

## Historical milestones

The original #36 tooling handoff was recorded against `ed76ecc82470b0d1504fd46f95e308c6e7c423f3` (PR #40). At that point it correctly stated that no Vietnamese target identity or source mapping had been selected. That statement describes the historical milestone only.

Issue #42 added the identity-agnostic `portion-study-manifest-0.2.0` compiler and `portion-measurement-0.1.0` protocol. Those remain deterministic and unchanged.

PR #52 later added the planning-stage `portion-study-manifest-0.3.0` binding and `portion-planning-packet-0.1.0`. The #52 packet remains immutable historical evidence of the state before the #53 strategy decision; it does not contain that later decision.

## Current reviewed food and strategy state

- `cơm trắng` identity and composition are reviewed through the exact FNDDS record in the #49 reviewed mapping.
- Evidence-identity binding is complete. The backend `FoodId` remains absent; no external FNDDS/FDC identifier is used as one.
- The owner approved Option A, a narrow reproducible pilot, in `github:issue/53#issuecomment-5884079108`.
- Option A applies only to one declared physical bowl/serving context. `generalizable_to_all_vietnamese_bat` is false. A broader household-use study requires separate versioned decision/evidence.
- The decision approves study strategy only. It does not approve a vessel, study-plan value, or gram amount.

The strategy decision is recorded at `vietnamese-com-trang-bat-study-strategy-decision-0.1.0.json`; the target-specific study-definition shell is `vietnamese-com-trang-bat-study-definition-0.1.0.json`.

## Remaining study-design blockers

All of the following remain explicitly unresolved. No examples, defaults, fixture values, or guessed numbers are supplied:

1. Exact vessel identifier/model or exact dimensional/capacity definition.
2. Vessel/context sampling rule within the narrow pilot.
3. Serving/fill protocol.
4. Minimum independent sample count.
5. Minimum independent cooking batch count.
6. Instrument requirements.
7. Calibration/check requirements.
8. Tare method.
9. Operator requirements.
10. Measurement date/session rules.
11. Measurement reviewer.
12. Deviation/exclusion policy.

Until each required design input has an explicit value and human review reference in a later version, `ready_for_measurement` remains false. The v0.1.0 study-definition shell is a planning record, not an executable measurement manifest.

## Observations, estimates, and publication

Future physical observations are represented separately from design decisions. No measurements have begun: `observation_count = 0`, `records = []`, `mass_estimate = null`, and `gram_estimate_emitted = false`.

Publication remains blocked: `review_ready = false`, `publishable = false`, `production_eligible = false`, `release_created = false`, and `activation_attempted = false`. No synthetic fixture `bát` grams are promoted.

## Measurement protocol boundary

The physical protocol remains `portion-measurement-0.1.0`, with estimator `mean_of_sample_means-0.1.0` and bounds `minmax_of_sample_means-0.1.0`. Independent-sample accounting, repeat-weighing semantics, instrument/calibration, tare, exact target/context matching, sample accounting, human review, and deterministic estimation remain governed by the existing compiler. No protocol or estimator behavior is weakened by the unresolved sample plan.

## Resume conditions

The domain owner/reviewers must supply and review all twelve study-design inputs above. A later version may then produce a measurement-execution manifest. Actual masses remain separate future observational evidence and must come from real reviewed measurements. Until then, the study is not ready for measurement and publication remains blocked.
