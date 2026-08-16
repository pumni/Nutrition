# VietnameseMealBench adjudication and release-gate protocol

`foundation-0.5.1` remains a development-only benchmark. The tooling in this
document prepares human review and produces aggregate evidence; it does not
approve annotations, create nutrition evidence, select a provider, or enable
production traffic.

## Annotation boundary

The benchmark has two separate gold layers:

1. Parse gold: item boundaries, food phrase, quantity, unit, preparation,
   modifiers, negation, and parser safety flags.
2. Analysis gold: food identity, portion, recipe, evidence resolution, and
   `resolve`/`needs_clarification`/`insufficient`.

Parse gold must be reviewed first. Analysis expectations remain provisional
until they record the catalog release, portion evidence release, and behavior
versions that make the decision reproducible. The phrase `trứng` must never be
silently resolved to `trứng gà`.

Each public case requires:

1. independent annotator A;
2. independent annotator B;
3. a named domain adjudicator for disagreements and final approval.

The agent may prepare packets and compare annotations, but it is not an
annotator or adjudicator. A comparison report with zero disagreements still
has status `awaiting_domain_adjudicator`.

## Preparing independent packets

Write packets outside the repository so annotations are not accidentally
committed:

~~~powershell
.\scripts\prepare-vietnamese-meal-bench-adjudication.ps1 -InputPath .\fixtures\vietnamese-meal-bench\public-test-cases.json -OutputDirectory C:\secure\vmb-foundation-0.5.1\packets
~~~

The generated packets contain case text and blank parse annotations only. They
do not copy the repository's expected parse or analysis answers. Assign the
two slots to independent humans through the approved external workflow.

After both packets are completed, compare them into an external report:

~~~powershell
.\scripts\compare-vietnamese-meal-bench-annotations.ps1 -AnnotatorAPath C:\secure\vmb-foundation-0.5.1\packets\independent-a.json -AnnotatorBPath C:\secure\vmb-foundation-0.5.1\packets\independent-b.json -OutputPath C:\secure\vmb-foundation-0.5.1\adjudication-queue.json
~~~

The queue contains only the disagreement set and both proposed annotations.
The comparator rejects blank or schema-invalid packets, checks exact text and
locale identity for each sample, rejects duplicate/empty safety flags, and
records SHA-256 hashes for both input packets. The queue must be reviewed by the domain adjudicator. The adjudicator's identity,
decision, rationale, guideline version, and review date belong in the
controlled external record. They must not be fabricated in this repository.

## Sealed and challenge mechanics

Sealed-test and challenge cases and answers are external-controlled. They
must not be copied into this repository, CI artifacts, logs, benchmark reports,
or provider prompts used for tuning.

The external release record must contain, at minimum:

- benchmark version and split name;
- case manifest hash and answer manifest hash;
- access-controlled storage reference;
- privacy/de-identification review reference for challenge data;
- named evaluator and evaluation date;
- behavior-version bundle;
- aggregate score report and release decision.

The repository manifest intentionally reports both split counts as
`external_not_loaded` and keeps `answers_in_repo=false`.

## Prediction and scoring contract

Predictions use `schemas/vietnamese-meal-bench-prediction-0.1.0.json`.
The scorer accepts either a JSON array or an object with a `predictions` array:

~~~json
{
  "sample_id": "vmb-public-0001",
  "parse_decision": "parsed",
  "items": [],
  "analysis_decision": "needs_clarification",
  "analysis_clarification_dimension": "food_identity",
  "safety_flags": []
}
~~~

Run the deterministic aggregate scorer with:

~~~powershell
.\scripts\score-vietnamese-meal-bench.ps1 -ExpectedPath .\fixtures\vietnamese-meal-bench\public-test-cases.json -PredictionsPath C:\secure\vmb-results\provider.json -SplitName public_test -OutputPath C:\secure\vmb-results\report.json
~~~

The current scorer reports schema validity, parse and analysis decision
accuracy, safety-flag exactness, normalized food-phrase mention precision/
recall/F1, over-resolution rate, and tag slices. Resolver top-k, nutrition,
calculation, and replay metrics remain `null` until the adjudicated expected artifacts
provide the required canonical IDs, evidence versions, or replay references. The
schema-valid rate is based on the exact versioned contract, including decision/
item constraints, field types, additional-property rejection, and unique safety
flags. Missing or invalid predictions remain in every expected-case and expected-
mention denominator. The report contains hashes and aggregates, not case text or
expected answers.

## Proposed release thresholds

The manifest's walking-skeleton thresholds are a proposal, not an approved
production policy:

| Metric | Proposed threshold |
|---|---:|
| Schema-valid parser output | >= 99% |
| Mention F1 | >= 0.90 |
| Known-food top-3 recall | >= 0.90 |
| Known-food top-1 accuracy | >= 0.78 |
| Unknown-detection precision | >= 0.85 |
| Over-resolution rate | <= 8% |
| Calculation fixture pass rate | 100% |
| Replay pass rate | 100% |

These numbers cannot make a release eligible by themselves. A release remains
blocked until independent human review, domain adjudication, version-pinned
analysis gold, sealed-test evidence, challenge privacy review, and slice-level
regression checks are complete. Any future change from proposal to effective
policy requires a separate owner decision.
