# Candidate release evidence inventory

`scripts/prepare-release-evidence.ps1` inventories source and build identity for a future
staging decision. It creates a candidate document only; it does not tag, publish, deploy,
activate catalog data, or change traffic. The approval and release fields in its output are
always `false`.

The input document must be outside the repository and must contain only these fields:

```json
{
  "catalog_release_id": "00000000-0000-7000-8000-000000000001",
  "catalog_release_evidence_ref": "staging-artifact://catalog-release/example",
  "parser_provider_version": "approved-provider/model-version",
  "container_images": [
    {
      "name": "api-http",
      "reference": "registry.example/nutrition-api",
      "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    {
      "name": "worker",
      "reference": "registry.example/nutrition-worker",
      "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }
  ]
}
```

Run it with an external, owner-provided evidence input:

```powershell
$inputs = Join-Path $env:TEMP "nutrition-release-inputs.json"
$output = Join-Path $env:TEMP "nutrition-release-evidence-candidate.json"
pwsh -NoLogo -NoProfile -File .\scripts\prepare-release-evidence.ps1 `
  -ReleaseInputsPath $inputs `
  -OutputPath $output
Get-Content -Raw $output
```

The tool fails before writing a candidate when the input is missing, has unknown fields,
contains an ambiguous catalog identifier, lacks a full container digest, contains a
secret-bearing value, or when the source tree is dirty. The output includes the application
version from the workspace, the bound Git commit, a SHA-256 inventory of forward migrations,
the parser schema and prompt constants, the owner-provided parser provider-version input, the
catalog evidence reference, container references/digests, and the hash of the external input
document. It never copies secrets or credentials into evidence.

This is an evidence preparation aid for issue #16. A human owner must still review the evidence,
staging behavior, security/privacy conditions, and release policy before any production action.

For the complete staging gate assembly, continue with
[STAGING_RELEASE_GATE.md](STAGING_RELEASE_GATE.md). That second step binds this candidate to the
six M0–M5 gate records and remains candidate-only.
