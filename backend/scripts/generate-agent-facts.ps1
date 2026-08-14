param(
    [string]$RepositoryRoot = (Get-Location).Path,
    [switch]$Check
)

$ErrorActionPreference = 'Stop'
$Root = [System.IO.Path]::GetFullPath($RepositoryRoot)

function Get-RepoPath([string]$RelativePath) {
    return [System.IO.Path]::Combine($Root, ($RelativePath -replace '/', [System.IO.Path]::DirectorySeparatorChar))
}

function Get-FileSha256([string]$RelativePath) {
    $path = Get-RepoPath $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "required fact input is missing: $RelativePath"
    }
    return (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Read-Json([string]$RelativePath) {
    return Get-Content -Raw -LiteralPath (Get-RepoPath $RelativePath) | ConvertFrom-Json
}

function Get-Provenance([string[]]$Paths) {
    return @($Paths | Sort-Object -Unique | ForEach-Object {
        [ordered]@{ path = $_; sha256 = Get-FileSha256 $_ }
    })
}

function Get-SourceFacts($Register) {
    $consumers = @{}
    foreach ($entry in $Register.PSObject.Properties) {
        foreach ($source in @($entry.Value)) {
            $path = [string]$source
            if (-not $consumers.ContainsKey($path)) { $consumers[$path] = [System.Collections.Generic.List[string]]::new() }
            $consumers[$path].Add([string]$entry.Name)
        }
    }
    return @($consumers.Keys | Sort-Object | ForEach-Object {
        [ordered]@{
            path = $_
            sha256 = Get-FileSha256 $_
            consumers = @($consumers[$_] | Sort-Object -Unique)
        }
    })
}

function New-FactDocument([string]$Artifact, $Provenance, $Facts) {
    $signatureInput = (@($Provenance | ForEach-Object { "$( [string]$_.path ):$( [string]$_.sha256 )" } | Sort-Object) -join "`n")
    $signatureBytes = [Text.Encoding]::UTF8.GetBytes($signatureInput)
    $signatureHash = [Security.Cryptography.SHA256]::Create().ComputeHash($signatureBytes)
    $signature = ([BitConverter]::ToString($signatureHash).Replace('-', '')).ToLowerInvariant()
    return [ordered]@{
        schema_version = '1.0.0'
        artifact = $Artifact
        generated_by = 'scripts/generate-agent-facts.ps1'
        generated_from = $Provenance
        refresh_attestation = [ordered]@{
            algorithm = 'SHA256'
            input_count = @($Provenance).Count
            signature = $signature
            consumer_refresh = 'required'
        }
        facts = $Facts
    }
}

$register = Read-Json '.agent/maps/source-register.json'
$sourceFacts = Get-SourceFacts $register
$sourcePaths = @($sourceFacts.path)
$documents = [ordered]@{
    'source-index.json' = New-FactDocument 'source-index' (Get-Provenance (@('.agent/maps/source-register.json', 'scripts/generate-agent-facts.ps1') + $sourcePaths)) ([ordered]@{
        source_register = '.agent/maps/source-register.json'
        sources = $sourceFacts
    })
}

$outputRoot = Get-RepoPath '.agent/generated'
if (-not (Test-Path -LiteralPath $outputRoot -PathType Container)) {
    if ($Check) { throw 'generated facts directory is missing' }
    New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
}

foreach ($name in $documents.Keys) {
    $outputPath = Join-Path $outputRoot $name
    $expected = (($documents[$name] | ConvertTo-Json -Depth 30) + [Environment]::NewLine)
    if ($Check) {
        if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) { throw "generated fact is missing: .agent/generated/$name" }
        $actual = Get-Content -Raw -LiteralPath $outputPath
        if ($actual -cne $expected) { throw "generated fact is stale: .agent/generated/$name" }
    } else {
        [System.IO.File]::WriteAllText($outputPath, $expected, [System.Text.UTF8Encoding]::new($false))
    }
}

if ($Check) {
    Write-Output '[PASS] Generated repository facts are fresh and deterministic.'
} else {
    Write-Output '[PASS] Generated repository facts written.'
}
