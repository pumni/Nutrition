param(
    [string]$RepositoryRoot = (Get-Location).Path,
    [switch]$Check
)

$ErrorActionPreference = "Stop"
$Root = [System.IO.Path]::GetFullPath($RepositoryRoot)

function Get-RepoPath([string]$RelativePath) {
    return [System.IO.Path]::Combine($Root, ($RelativePath -replace '/', [System.IO.Path]::DirectorySeparatorChar))
}

function Convert-ToRepoPath([string]$Path) {
    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $rootPrefix = $Root.TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $fullPath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "path is outside repository root: $Path"
    }
    return $fullPath.Substring($rootPrefix.Length).Replace([System.IO.Path]::DirectorySeparatorChar, '/')
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

function Get-CanonicalJson($Value) {
    return (($Value | ConvertTo-Json -Depth 30) + [Environment]::NewLine)
}

function Get-PackageFacts($Metadata) {
    return @($Metadata.packages | Sort-Object name, version | ForEach-Object {
        $package = $_
        $targets = @($package.targets | Sort-Object name | ForEach-Object {
            [ordered]@{
                name = [string]$_.name
                kinds = @($_.kind | Sort-Object)
                crate_types = @($_.crate_types | Sort-Object)
            }
        })
        $dependencies = @($package.dependencies | Sort-Object name, kind, optional, req | ForEach-Object {
            [ordered]@{
                name = [string]$_.name
                req = [string]$_.req
                kind = if ($null -eq $_.kind) { "normal" } else { [string]$_.kind }
                optional = [bool]$_.optional
                target = if ($null -eq $_.target) { "" } else { [string]$_.target }
            }
        })
        [ordered]@{
            name = [string]$package.name
            version = [string]$package.version
            manifest_path = Convert-ToRepoPath $package.manifest_path
            targets = $targets
            dependencies = $dependencies
        }
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
    return @($consumers.Keys | Sort-Object | Where-Object { $_ -notlike '.agent/generated/*' } | ForEach-Object {
        [ordered]@{
            path = $_
            sha256 = Get-FileSha256 $_
            consumers = @($consumers[$_] | Sort-Object -Unique)
        }
    })
}

function Get-RustFiles {
    $cratesRoot = Get-RepoPath 'crates'
    if (-not (Test-Path -LiteralPath $cratesRoot -PathType Container)) { return @() }
    return @(Get-ChildItem -LiteralPath $cratesRoot -Recurse -File -Filter '*.rs' | Sort-Object FullName)
}

function Get-TestFacts($RustFiles) {
    return @($RustFiles | ForEach-Object {
        $relative = Convert-ToRepoPath $_.FullName
        $text = Get-Content -Raw -LiteralPath $_.FullName
        $testCount = @([regex]::Matches($text, '(?m)^\s*#\[(?:tokio::)?test\]\s*$')).Count
        $integration = $relative -like '*/tests/*'
        if ($testCount -gt 0 -or $integration) {
            [ordered]@{
                path = $relative
                sha256 = Get-FileSha256 $relative
                test_attributes = $testCount
                integration_test_file = $integration
            }
        }
    })
}

function Get-RouteFacts($RustFiles) {
    $routes = @()
    foreach ($file in $RustFiles) {
        $relative = Convert-ToRepoPath $file.FullName
        $text = Get-Content -Raw -LiteralPath $file.FullName
        foreach ($match in [regex]::Matches($text, '\.route\(\s*"([^"]+)"')) {
            $routes += [ordered]@{ path = $relative; route = [string]$match.Groups[1].Value }
        }
    }
    return @($routes | Sort-Object path, route)
}

function Get-SchemaFacts {
    $schemaRoot = Get-RepoPath 'schemas'
    if (-not (Test-Path -LiteralPath $schemaRoot -PathType Container)) { return @() }
    return @(Get-ChildItem -LiteralPath $schemaRoot -Recurse -File -Filter '*.json' | Sort-Object FullName | ForEach-Object {
        $relative = Convert-ToRepoPath $_.FullName
        $document = Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json
        [ordered]@{
            path = $relative
            sha256 = Get-FileSha256 $relative
            schema_id = if ($null -eq $document.'$id') { "" } else { [string]$document.'$id' }
            title = if ($null -eq $document.title) { "" } else { [string]$document.title }
        }
    })
}

function New-FactDocument([string]$Artifact, $Provenance, $Facts) {
    return [ordered]@{
        schema_version = '1.0.0'
        artifact = $Artifact
        generated_by = 'scripts/generate-agent-facts.ps1'
        generated_from = $Provenance
        facts = $Facts
    }
}

$register = Read-Json '.agent/maps/source-register.json'
$cargoManifest = Get-RepoPath 'Cargo.toml'
$metadataText = & cargo metadata --no-deps --format-version 1 --manifest-path $cargoManifest 2>$null | Out-String
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed while generating repository facts' }
$metadata = $metadataText | ConvertFrom-Json
$rustFiles = Get-RustFiles
$sourceFacts = Get-SourceFacts $register
$sourcePaths = @($sourceFacts.path)
$cargoInputs = @('Cargo.toml')
if (Test-Path -LiteralPath (Get-RepoPath 'Cargo.lock') -PathType Leaf) { $cargoInputs += 'Cargo.lock' }
$testInputs = @($rustFiles | ForEach-Object { Convert-ToRepoPath $_.FullName })
$routeInputs = @($testInputs) + @('schemas/parsed-meal-0.1.0.json')
$registerInputs = @('.agent/maps/source-register.json') + $sourcePaths

$documents = [ordered]@{
    'crate-graph.json' = New-FactDocument 'crate-graph' (Get-Provenance (@($cargoInputs) + 'scripts/generate-agent-facts.ps1')) ([ordered]@{
        workspace_members = @($metadata.packages | Where-Object { $metadata.workspace_members -contains $_.id } | Sort-Object name | ForEach-Object { [string]$_.name })
        packages = Get-PackageFacts $metadata
    })
    'source-index.json' = New-FactDocument 'source-index' (Get-Provenance (@('.agent/maps/source-register.json', 'scripts/generate-agent-facts.ps1') + $sourcePaths)) ([ordered]@{
        source_register = '.agent/maps/source-register.json'
        derived_artifacts_excluded = '.agent/generated/**'
        sources = $sourceFacts
    })
    'test-map.json' = New-FactDocument 'test-map' (Get-Provenance (@('scripts/generate-agent-facts.ps1') + $testInputs)) ([ordered]@{
        files = Get-TestFacts $rustFiles
    })
    'change-impact-map.json' = New-FactDocument 'change-impact-map' (Get-Provenance (@('.agent/maps/source-register.json', 'Cargo.toml', 'scripts/generate-agent-facts.ps1') + $sourcePaths)) ([ordered]@{
        path_ownership = @($sourceFacts | Where-Object { $_.path -like 'crates/*' } | ForEach-Object {
            $parts = $_.path.Split('/')
            [ordered]@{ path = $_.path; package = if ($parts.Count -gt 1) { $parts[1] } else { '' }; consumers = $_.consumers }
        })
        route_inventory = Get-RouteFacts $rustFiles
        schema_inventory = Get-SchemaFacts
    })
}

$outputRoot = Get-RepoPath '.agent/generated'
if (-not (Test-Path -LiteralPath $outputRoot -PathType Container)) {
    if ($Check) { throw 'generated facts directory is missing' }
    New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
}

foreach ($name in $documents.Keys) {
    $outputPath = Join-Path $outputRoot $name
    $expected = Get-CanonicalJson $documents[$name]
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
