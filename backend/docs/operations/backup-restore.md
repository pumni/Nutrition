# Backup, restore, and rollback drill

Status: staging-preparation contract; production activation is not authorized.

PostgreSQL recovery uses encrypted daily backups and
continuous WAL/PITR where the selected platform supports it, with a 15-minute RPO objective,
4-hour RTO objective, 35-day backup retention, monthly staging restore drills, and at least
quarterly restore drills from production backup copies. The platform owner must still configure
the actual backup service, encryption keys, storage, access controls, WAL archive, and secret
manager. The repository does not guess those infrastructure choices.

## Isolated local/staging-safe drill

The deterministic drill script uses only the repository's local PostgreSQL Compose service as a
source and a fresh, disposable PostgreSQL container as the restore target. It writes both the
custom-format backup and JSON evidence outside the repository. It rejects repository paths for
those artifacts and contains no production connection-string input.

```powershell
$evidence = Join-Path $env:TEMP "nutrition-recovery-evidence.json"
$backup = Join-Path $env:TEMP "nutrition-recovery.dump"
$privacy = Join-Path $env:TEMP "nutrition-recovery-privacy-replay.json"
pwsh -NoLogo -NoProfile -File .\scripts\run-backup-restore-drill.ps1 `
  -OutputPath $evidence `
  -BackupPath $backup `
  -PrivacyReplayManifestPath $privacy `
  -InitializeFoundation
Get-Content -Raw $evidence
```

The privacy manifest is an external, non-sensitive platform artifact. For this synthetic fixture,
it must use `schema_version=privacy-restore-gate-0.1.0`, `environment=synthetic-local`,
`replay_status=applied`, `tombstones_applied=true`, `production_authorization=false`, a safe
`replay_reference`, and zero deletion/retention tombstones. The script verifies the manifest before
it starts the API. A staging run may use `environment=staging` and must provide the platform's real
replay result and matching counts; the script does not invent a tombstone table or silently serve
restored data when the replay gate is absent.

The script performs the following bounded checks:

- starts a fresh disposable local PostgreSQL Compose project with the pinned `postgres:18` image;
- creates a PostgreSQL custom-format dump without placing it in the repository;
- encrypts the external local drill artifact with an ephemeral AES-256 key, emits no key, and
  removes plaintext scratch copies after transfer;
- restores the dump into a new isolated PostgreSQL 18 container;
- compares migration version/checksum inventory, schema/constraint fingerprints, data fingerprints,
  analysis, catalog, audit, job, and outbox snapshots;
- verifies no restored analysis row contains `raw_text_ciphertext`;
- verifies the privacy replay manifest before serving restored data;
- starts the API against the restored database and checks HTTP 200 readiness, exact owner-scoped
  listing fields/count after the API's timestamp snapshot boundary, owner detail access, and
  foreign-owner list/detail isolation;
- records backup/restore duration, artifact SHA-256, and objective comparisons;
- removes the disposable restore container and deletes only the source project/volume it created.

The observed local RPO is reported as zero rows omitted from the logical backup snapshot. This is
useful evidence for the tested dump/restore boundary, but it is not a claim that the platform's
continuous WAL/PITR archive has been tested. The measured local RTO is the end-to-end duration
through restore and API checks; it must be reviewed against the 4-hour objective together with
platform-specific staging evidence.

## Platform implementation requirements

Before staging is considered recoverable, the platform owner must bind the following to the
candidate release and retain the references in the release evidence:

1. Backup job schedule, WAL/PITR archive health, encryption-at-rest and in-transit settings.
2. Backup retention policy of 35 days and access restricted to the recovery operators.
3. A disposable restore target isolated from production traffic and credentials.
4. A restore sequence that reapplies deletion and retention tombstones before serving any
   restored user data. Deleted user data may remain in encrypted backups until expiry. The
   repository's current privacy implementation hard-deletes user-owned aggregates and retains a
   bounded audit event; a platform-specific tombstone/replay mechanism must be supplied and
   recorded at staging rather than invented by this repository task.
5. Evidence of the measured RPO/RTO, application-level checks, and the exact backup artifact or
   platform run reference.

## Rollback boundaries

The versioned plan is [deploy/recovery/rollback-plan.json](../../deploy/recovery/rollback-plan.json).
Application rollback selects the prior accepted image digest and reviewed configuration
fingerprint. It never copies secret values into evidence. Database recovery is forward-only:
restore a compatible backup and deploy the compatible application; do not mutate or reverse
applied migrations.

Catalog rollback creates a new staged immutable snapshot from the prior superseded release through
the existing catalog rollback workflow. It recomputes the manifest checksum, validates membership
and evidence, and uses the explicit catalog activation gate. A synthetic rollback evidence input
can be validated without deployment:

```powershell
$rollbackInput = Join-Path $env:TEMP "nutrition-recovery-rollback-input.json"
$rollbackEvidence = Join-Path $env:TEMP "nutrition-recovery-rollback-evidence.json"
pwsh -NoLogo -NoProfile -File .\scripts\validate-recovery-rollback.ps1 `
  -InputPath $rollbackInput -OutputPath $rollbackEvidence
```

The input must bind the current `deploy/recovery/rollback-plan.json` SHA-256, the deterministic
migration inventory SHA-256, and an external catalog rollback manifest. That manifest must be a
checksummed `catalog-rollback-manifest-0.1.0` record with `source_status=superseded`,
`validation_status=verified`, a positive membership count, and `activation_performed=false`.
Application image/configuration references remain external evidence and must be immutable SHA-256
values; the validator checks their shape and that the rollback target differs from the current
values, but it does not claim a deployment occurred.

This validates immutable application image/configuration references, forward-only migration
compatibility, and a staged immutable catalog rollback reference. It does not deploy an image,
activate a catalog, change traffic, call the hosted provider, or authorize a release. Platform
staging evidence must still bind the actual image digests, configuration fingerprints, and catalog
rollback execution.

## Required review before production

The drill artifact alone does not authorize production. Production remains blocked until provider
privacy, benchmark, `production_eligible` catalog evidence, staging SLO/load/restore review, and
release/rollback gates are all accepted by the owner.
