# Staging release gate evidence

Status: candidate-evidence preparation only. Production publication, production traffic, and
catalog activation are not authorized.

The current passed-artifact wrapper is `staging-gate-evidence-wrapper-0.2.0`; the assembled gate
output is `staging-release-gate-evidence-0.2.0`. These versions reflect the removal of historical
task provenance fields. Consumers must reject an older shape unless they explicitly migrate it.

`scripts/prepare-staging-release-gate.ps1` assembles a deterministic, fail-closed gate record for
an exact candidate. It consumes two external JSON documents:

1. candidate evidence from `scripts/prepare-release-evidence.ps1`;
2. a gate-input document containing the external staging/auth/review evidence and the six required
   M0–M5 gate statuses.

All input and output paths must be outside the repository. The script binds the candidate file
SHA-256, current clean Git commit, migration set, parser/schema/provider identity, catalog release,
behavior vector, externally hashed auth configuration evidence, externally hashed rollback-target
evidence, container digests, and the versioned rollback plan SHA-256. It never reads or writes secrets, deploys an image, calls a hosted provider, changes
traffic, activates a catalog, creates a tag, or publishes `v1.0.0`.

## Required gates

The gate input must contain exactly one record for each ID below. A `pass` record requires an
external artifact path; the script computes its SHA-256 and compares it with the declared digest,
then validates a wrapper containing the gate ID, candidate SHA, subject commit, result, scope, and
`production_authorization=false`. A `blocked` record must carry a safe evidence reference but no
artifact or waiver. Waived records are rejected by this gate; an exceptional release decision must
be handled by a separately reviewed, versioned process.

- `M0-governance`
- `M1-provider-privacy`
- `M2-vietnamese-benchmark`
- `M3-catalog-production-eligibility`
- `M4-staging-slo-load-restore`
- `M5-release-rollback`

The output is `status=blocked` when any gate is blocked. When all six gates are closed, the output
is only `ready_for_owner_release_review`; it is never a release approval.

## Candidate preparation

```powershell
$candidateInput = Join-Path $env:TEMP "nutrition-release-inputs.json"
$candidate = Join-Path $env:TEMP "nutrition-release-evidence-candidate.json"
pwsh -NoLogo -NoProfile -File .\scripts\prepare-release-evidence.ps1 `
  -ReleaseInputsPath $candidateInput -OutputPath $candidate
```

The candidate input must be owner/platform evidence. Do not replace missing catalog, provider,
auth, or container evidence with guessed values. A foundation fixture may be used for a synthetic
local drill only and is not production-eligible.

## Gate assembly

```powershell
$gateInputs = Join-Path $env:TEMP "nutrition-staging-gate-input.json"
$gateEvidence = Join-Path $env:TEMP "nutrition-staging-gate-evidence.json"
pwsh -NoLogo -NoProfile -File .\scripts\prepare-staging-release-gate.ps1 `
  -CandidateEvidencePath $candidate `
  -GateInputsPath $gateInputs `
  -OutputPath $gateEvidence
Get-Content -Raw $gateEvidence
```

The gate record is the durable handoff for owner review. Production still requires provider privacy,
benchmark, `production_eligible` catalog evidence, staging SLO/load/restore evidence, and reviewed
release/rollback targets before activation. The owner remains the sole authority for canonical
publication and production release.
