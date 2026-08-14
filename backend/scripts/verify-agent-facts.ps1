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

$names = @('crate-graph.json', 'source-index.json', 'test-map.json', 'change-impact-map.json')
foreach ($name in $names) {
    $path = Join-Path $Root (Join-Path '.agent/generated' $name)
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "generated fact is missing: .agent/generated/$name"
    }
    $document = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
    if ($document.schema_version -ne '1.0.0') { throw "generated fact schema version is invalid: $name" }
    if ($document.artifact -ne [System.IO.Path]::GetFileNameWithoutExtension($name)) { throw "generated fact identity is invalid: $name" }
    if ($document.generated_by -ne 'scripts/generate-agent-facts.ps1') { throw "generated fact provenance is invalid: $name" }
    if (@($document.generated_from).Count -eq 0) { throw "generated fact has no provenance: $name" }
    if ($null -eq $document.facts) { throw "generated fact has no facts payload: $name" }
}

& $generator -RepositoryRoot $Root -Check
if ($LASTEXITCODE -ne 0) { throw 'generated facts freshness check failed' }
Write-Output '[PASS] Generated repository fact inventory verified.'
