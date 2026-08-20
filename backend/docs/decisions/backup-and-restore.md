# ADR: Backup and restore targets

**Status:** Accepted operational target.

## Decision

PostgreSQL uses encrypted daily backups and continuous WAL/PITR where supported, with a 15-minute
RPO, four-hour RTO, 35-day retention, and recurring staging/production restore drills. Catalog and
source packages remain checksum-bound and immutable; privacy deletion/retention tombstones are
reapplied before restored user data serves traffic.

## Evidence / affected paths

- `docs/operations/backup-restore.md`
- `scripts/run-backup-restore-drill.ps1`
