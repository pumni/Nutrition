# VietnameseMealBench adjudication and release-gate protocol

`foundation-0.5.1` remains a development-only benchmark. The tooling in this
document prepares human review and produces aggregate evidence; it does not
approve annotations, create nutrition evidence, select a provider, or enable
production traffic.

## Operator sequence

Run the phases below in order. The repository supplies fixtures, schemas, and
deterministic tooling; human reviewers supply the decisions and release
evidence. Keep the working directory for packets, annotations, adjudication,
sealed/challenge material, predictions, and reports outside the repository.

1. **Structural preflight.** Run `scripts/verify-vietnamese-meal-bench.ps1`
   against the checked-out release. Confirm that the manifest remains
   `development-only`, public annotations remain `pending_human_review`,
   analysis gold remains `provisional`, thresholds remain
   `proposal_not_approved`, and sealed/challenge answers are external.
2. **Independent packet generation.** Run
   `scripts/prepare-vietnamese-meal-bench-adjudication.ps1` once for the public
   cases. Record the input case-file SHA-256 and the generated packet hashes.
   Assign packet A and packet B to independent annotators without sharing
   annotations between them.
3. **Independent parse annotation.** Each annotator completes only the parse
   layer in the packet, using
   `schemas/vietnamese-meal-bench-annotation-0.1.0.json`. They record the
   parser decision, extracted items, and safety flags; they do not invent food
   identity, portion mass, nutrient values, or analysis outcomes.
4. **Comparison and adjudication.** Run
   `scripts/compare-vietnamese-meal-bench-annotations.ps1` and preserve its
   queue and both input hashes. A non-empty disagreement set goes to the named
   domain adjudicator. Even a zero-disagreement comparison remains
   `awaiting_domain_adjudicator` until that person records the decision,
   rationale, guideline version, identity, and review date in the external
   record.
5. **Analysis-gold pinning.** After parse gold is approved, the domain owner
   records analysis expectations separately. Every approved analysis decision
   must identify the catalog release, portion-evidence release, recipe/source
   evidence where relevant, and complete behavior-version vector. Until those
   references are reproducible and approved, analysis gold remains provisional.
6. **Sealed/challenge release evidence.** The evaluator prepares access-controlled
   manifests for the externally held cases and answers. Record manifest hashes,
   storage references, evaluator identity/date, and challenge privacy/
   de-identification review. Do not copy case text or answers into this
   repository, CI artifacts, benchmark reports, or provider prompts.
7. **Prediction scoring.** Validate provider predictions against
   `schemas/vietnamese-meal-bench-prediction-0.1.0.json`, then run
   `scripts/score-vietnamese-meal-bench.ps1`. Keep the aggregate report and
   prediction hash outside the repository. Missing or schema-invalid
   predictions remain failures in the denominators; they are not omitted.
8. **Release-gate review.** Assemble the external release record and run the
   deterministic benchmark regression test plus the repository verification
   gates. A passing structural or aggregate report is evidence for the owner;
   it does not approve thresholds, a provider, catalog evidence, or production
   traffic.

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

## External artifact and retention checklist

The operator should maintain an access-controlled evidence directory with at
least these records:

| Artifact | Required metadata | Repository rule |
|---|---|---|
| Public case input and manifest | version, file SHA-256, split name | The checked-in fixture remains the source; do not rewrite expected answers during review. |
| Annotator A/B packets | annotator slot, input hash, completed packet hash, completion date | Keep outside the repository; packets contain parse annotations only. |
| Comparison and adjudication record | both packet hashes, disagreement set, adjudicator identity, decision, rationale, guideline version, date | Keep external; zero disagreements still require adjudicator disposition. |
| Analysis-gold record | catalog release, portion-evidence release, behavior versions, evidence references, owner approval | Keep provisional until every required reference is reproducible and approved. |
| Sealed/challenge release record | case/answer manifest hashes, access-controlled URI, evaluator/date, challenge privacy review | Never store case files or answers in the repository or CI artifacts. |
| Prediction and aggregate reports | prediction hash, schema version, split, scorer version, aggregate metrics, report hash | Store hashes and aggregates; do not publish sealed answers or case text. |

Retain the external records according to the approved access, privacy, and
release-retention policy. The repository does not define that policy and the
agent must not invent retention periods or access grants.

## Failure states and stop conditions

- A structural preflight failure stops the run; do not generate packets from a
  manifest with changed status, counts, hashes, or release gates.
- A blank, malformed, duplicate, text-mismatched, locale-mismatched, or
  otherwise invalid packet is rejected by the comparator and must be corrected
  by the responsible annotator. Do not edit it to bypass validation.
- Any disagreement, including a disagreement hidden by normalization, remains
  in the adjudication queue until the domain adjudicator disposes of it.
- Missing catalog, portion, recipe/source, or behavior-version references block
  analysis-gold pinning; parse approval alone does not make analysis gold
  production-eligible.
- Missing access control or privacy review blocks sealed/challenge evidence.
- Missing or schema-invalid predictions block scoring acceptance. A score
  report with proposed thresholds is evidence only while
  `threshold_status=proposal_not_approved`.
- Any sealed/challenge answer found in the repository, CI artifact, log, report,
  or provider prompt is a release incident: stop evaluation and notify the
  owner. Do not delete or rewrite evidence to hide the exposure.
- No benchmark result in this runbook authorizes `production_eligible=true`, a
  hosted provider, catalog activation, production traffic, or a release tag.

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

## Final external release-record minimum

Before an owner considers the benchmark gate, the external record must link or
hash all of the following:

- the exact benchmark version, public case manifest, and answer manifest;
- both independent annotation packets and the comparison queue;
- the named adjudicator's decision and guideline version;
- the approved analysis-gold evidence versions and behavior-version bundle;
- sealed-test and challenge access-control evidence, including challenge privacy
  review;
- evaluator identity/date, prediction hash, aggregate score report, slice
  regressions, and scorer version;
- the separate owner decision for threshold policy and the resulting release
  decision.

The final record may conclude that the gate is blocked. A complete record is
not itself production authorization; provider, catalog, operations, staging,
and release gates remain independent human decisions.
