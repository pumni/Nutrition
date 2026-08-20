---
name: release-check
description: Gather repository release evidence without activating production data or publishing a release.
---

# Release check

Read [docs/operations/staging-release-gate.md](../../../docs/operations/staging-release-gate.md), the current release
notes, and the source register. Run only the checks relevant to the proposed release and preserve
provenance, hashes, and reviewer evidence. Report missing gates explicitly. Do not activate a
catalog, enable a provider, deploy production, or publish release metadata without an explicit
human-controlled operation.
