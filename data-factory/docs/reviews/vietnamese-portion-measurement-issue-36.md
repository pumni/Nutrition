# Vietnamese portion measurement tooling handoff

Status: `tooling_only_publication_blocked`

Baseline: `main` at `ed76ecc82470b0d1504fd46f95e308c6e7c423f3` (PR #40).

This packet records the issue #36 tooling boundary. No Vietnamese measurement target, food identity,
source mapping, or physical observation is selected or published here.

## Coordination and current blockers

- Issue #35 remains open and blocked on the owner/source decision in #41. Issue #41 has no decision
  comment in the current baseline check.
- The latest #36 coordination permits only identity-agnostic measurement tooling. It forbids binding
  contexts such as `cơm trắng × bát` or `trứng luộc × quả` to a catalog identity before reviewed
  identity semantics exist.
- The pinned coverage snapshot reports zero published portion observations and seven measurement
  queue candidates. This work does not promote any queue item or add measurement values.
- Genuine physical measurements, calibration records, tare data, target review, and measurement
  reviewer approval are absent. No candidate measurement release is created.

## Tooling added

- Study manifest schema: `schemas/portion-study-manifest-0.2.0.json`.
- Measurement document schema: `schemas/portion-measurements-0.1.0.json`.
- Validator/compiler: `src/nutrition_data_factory/portion.py`.
- Human packet command: `scripts/prepare_portion_review_packet.py`.

Manifest schema `0.2.0` is a narrow successor to `0.1.0`: it adds explicit human review states for
identity/preparation/context, a reviewed sample/batch plan, and numeric calibration/tare checks.
The physical measurement protocol remains `portion-measurement-0.1.0`; the successor does not relax
identity matching or enable publication.

The study plan carries human-reviewed minimum independent-sample and batch counts. The tool does not
choose sample counts. Each observation binds to the exact reviewed identity, preparation, measure,
represented quantity, and physical context from its manifest. Unresolved identity/preparation/context
prevents mass estimation. Repeats must point to a primary independent-sample weighing and do not
increase the independent sample or batch count.

`mass_g` is the net portion mass after applying the recorded tare. The manifest pins instrument
resolution, a dated calibration check and tolerance, tare method/mass, and the estimator/bound-policy
versions. The deterministic estimator averages readings within each independent sample, gives each
independent sample equal weight for the central estimate, and uses the minimum/maximum sample mean as
the lower/upper bounds. Raw manifest and measurement file bytes are retained by SHA-256 in the local
content-addressed store; the review packet records those artifact references and the compiler hash.

The packet tool has no backend/database or release-activation dependency. It always reports
`publication.status = blocked`, `publishable = false`, `release_created = false`, and
`activation_attempted = false`. Unknown fields such as parser-suggested `gram_weight`, invalid masses,
context mismatches, incomplete plans, or non-human reviewer markers fail closed.

## Resume conditions

Before selecting or binding Vietnamese measurement targets:

1. Resolve #35/#41 with an explicit reviewed food identity, preparation, and permitted source
   strategy, or leave that food unsupported.
2. Have a domain reviewer approve each physical measure context and the target-specific sample/batch
   plan.
3. Collect actual measurements with instrument, calibration, tare, operator, timestamp, sample, and
   repeat metadata. Do not fill these fields from examples or model output.
4. Generate a hash-pinned review packet and obtain a separate human measurement review.

Until those inputs exist, issue #36 remains tooling-only and publication-blocked. Recipe evidence is
outside this issue's scope.
