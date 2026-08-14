param(
    [string]$RepositoryRoot = (Get-Location).Path,
    [switch]$Check
)

$ErrorActionPreference = 'Stop'
$Root = [System.IO.Path]::GetFullPath($RepositoryRoot)
$generator = Join-Path $Root 'scripts/generate-agent-facts.ps1'

if (-not (Test-Path -LiteralPath $generator -PathType Leaf)) {
    throw 'generated facts verifier cannot find scripts/generate-agent-facts.ps1'
}

$name = 'source-index.json'
$path = Join-Path $Root (Join-Path '.agent/generated' $name)
if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "generated fact is missing: .agent/generated/$name"
}
$document = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
if ($document.schema_version -ne '1.0.0') { throw "generated fact schema version is invalid: $name" }
if ($document.artifact -ne 'source-index') { throw "generated fact identity is invalid: $name" }
if ($document.generated_by -ne 'scripts/generate-agent-facts.ps1') { throw "generated fact provenance is invalid: $name" }
if (@($document.generated_from).Count -eq 0) { throw "generated fact has no provenance: $name" }
if ($null -eq $document.refresh_attestation -or $document.refresh_attestation.algorithm -ne 'SHA256' -or $document.refresh_attestation.consumer_refresh -ne 'required') { throw "generated fact refresh attestation is missing: $name" }
if ([int]$document.refresh_attestation.input_count -ne @($document.generated_from).Count -or [string]$document.refresh_attestation.signature -notmatch '^[0-9a-fA-F]{64}$') { throw "generated fact refresh attestation is invalid: $name" }
$signatureInput = (@($document.generated_from | ForEach-Object { "$( [string]$_.path ):$( [string]$_.sha256 )" } | Sort-Object) -join "`n")
$signatureHash = [Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($signatureInput))
$signature = ([BitConverter]::ToString($signatureHash).Replace('-', '')).ToLowerInvariant()
if ($signature -ne ([string]$document.refresh_attestation.signature).ToLowerInvariant()) { throw "generated fact refresh attestation does not match inputs: $name" }
if ($null -eq $document.facts -or $null -eq $document.facts.sources) { throw "generated source index has no sources payload: $name" }

$null = & $generator -RepositoryRoot $Root -Check
if (-not $?) { throw 'generated facts freshness check failed' }
Write-Output '[PASS] Generated repository fact inventory verified.'
