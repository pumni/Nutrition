[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ReleaseInputsPath,
    [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$repositoryPrefix = $repositoryRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar

function Assert-OutsideRepository {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Label)
    $fullPath = [IO.Path]::GetFullPath($Path)
    if ($fullPath.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label must be outside the repository"
    }
    return $fullPath
}

function Require-Text {
    param([Parameter(Mandatory = $true)][object]$Object, [Parameter(Mandatory = $true)][string]$Name, [Parameter(Mandatory = $true)][string]$Label)
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property -or $null -eq $property.Value -or [string]::IsNullOrWhiteSpace([string]$property.Value)) {
        throw "$Label is missing required field '$Name'"
    }
    return [string]$property.Value
}

function Assert-SafeEvidenceText {
    param([Parameter(Mandatory = $true)][string]$Value, [Parameter(Mandatory = $true)][string]$Label)
    if ($Value -match '(?i)(password|secret|token|api[_-]?key|authorization|bearer|private[_-]?key|database[_-]?url)') {
        throw "$Label contains a prohibited secret-bearing value"
    }
}

function Get-Sha256ForText {
    param([Parameter(Mandatory = $true)][string]$Value)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Value))).Replace('-', '')).ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

function Get-GitValue {
    param([Parameter(Mandatory = $true)][string[]]$Arguments, [Parameter(Mandatory = $true)][string]$Label)
    $value = (& git -c core.excludesFile= @Arguments 2>$null | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($value)) {
        throw "could not resolve $Label"
    }
    return $value
}

function Get-Constant {
    param([Parameter(Mandatory = $true)][string]$Source, [Parameter(Mandatory = $true)][string]$Name, [Parameter(Mandatory = $true)][string]$Label)
    $pattern = 'pub const ' + [regex]::Escape($Name) + '\s*:\s*&str\s*=\s*"([^"]+)"'
    $match = [regex]::Match($Source, $pattern)
    if (-not $match.Success -or [string]::IsNullOrWhiteSpace($match.Groups[1].Value)) {
        throw "$Label constant $Name is missing or ambiguous"
    }
    return $match.Groups[1].Value
}

$inputsFullPath = Assert-OutsideRepository $ReleaseInputsPath "ReleaseInputsPath"
$outputFullPath = Assert-OutsideRepository $OutputPath "OutputPath"
if (-not (Test-Path -LiteralPath $inputsFullPath -PathType Leaf)) {
    throw "ReleaseInputsPath does not exist: $inputsFullPath"
}

$rawInputs = Get-Content -Raw -LiteralPath $inputsFullPath
Assert-SafeEvidenceText $rawInputs "release input document"
try {
    $inputs = $rawInputs | ConvertFrom-Json
}
catch {
    throw "ReleaseInputsPath is not valid JSON"
}

$allowedInputProperties = @(
    "catalog_release_id",
    "catalog_release_evidence_ref",
    "parser_provider_version",
    "container_images"
)
foreach ($property in $inputs.PSObject.Properties.Name) {
    if ($property -notin $allowedInputProperties) {
        throw "release input contains unknown field '$property'"
    }
}

$catalogReleaseId = Require-Text $inputs "catalog_release_id" "release input"
$parsedCatalogReleaseId = [Guid]::Empty
if (-not [Guid]::TryParse($catalogReleaseId, [ref]$parsedCatalogReleaseId)) {
    throw "catalog_release_id must be an unambiguous UUID"
}
$catalogEvidenceRef = Require-Text $inputs "catalog_release_evidence_ref" "release input"
$parserProviderVersion = Require-Text $inputs "parser_provider_version" "release input"
Assert-SafeEvidenceText $catalogEvidenceRef "catalog_release_evidence_ref"
Assert-SafeEvidenceText $parserProviderVersion "parser_provider_version"

$containerProperty = $inputs.PSObject.Properties["container_images"]
if ($null -eq $containerProperty -or $null -eq $containerProperty.Value -or @($containerProperty.Value).Count -eq 0) {
    throw "release input must contain at least one container_images entry"
}
$containerImages = [Collections.Generic.List[object]]::new()
$imageNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($image in @($containerProperty.Value)) {
    $name = Require-Text $image "name" "container image"
    $reference = Require-Text $image "reference" "container image $name"
    $digest = Require-Text $image "digest" "container image $name"
    Assert-SafeEvidenceText $name "container image name"
    Assert-SafeEvidenceText $reference "container image reference"
    Assert-SafeEvidenceText $digest "container image digest"
    if (-not $imageNames.Add($name)) {
        throw "container image name is duplicated: $name"
    }
    if ($digest -notmatch '^sha256:[0-9a-fA-F]{64}$') {
        throw "container image $name digest must be a full sha256 digest"
    }
    $containerImages.Add([ordered]@{
        name = $name
        reference = $reference
        digest = $digest.ToLowerInvariant()
    })
}

$gitCommit = Get-GitValue @("rev-parse", "--verify", "HEAD") "Git commit"
if ($gitCommit -notmatch '^[0-9a-fA-F]{40}$') {
    throw "Git commit is not a full commit SHA"
}
$gitStatus = (& git -c core.excludesFile= status --porcelain --untracked-files=all 2>$null | Out-String).Trim()
if (-not [string]::IsNullOrWhiteSpace($gitStatus)) {
    throw "working tree is not clean; release evidence must bind to a clean source tree"
}

$cargoPath = Join-Path $repositoryRoot "Cargo.toml"
$cargoText = Get-Content -Raw -LiteralPath $cargoPath
$versionMatch = [regex]::Match($cargoText, '(?ms)\[workspace\.package\].*?^version\s*=\s*"([^"]+)"')
if (-not $versionMatch.Success) {
    throw "workspace application version is missing or ambiguous"
}
$applicationVersion = $versionMatch.Groups[1].Value

$migrationDirectory = Join-Path $repositoryRoot "migrations"
$migrationFiles = @(Get-ChildItem -LiteralPath $migrationDirectory -File -Filter "*.sql" | Sort-Object Name)
if ($migrationFiles.Count -eq 0) {
    throw "no forward migration files were found"
}
$migrationInventory = [Collections.Generic.List[object]]::new()
$migrationLines = [Collections.Generic.List[string]]::new()
foreach ($migration in $migrationFiles) {
    $digest = (Get-FileHash -LiteralPath $migration.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $migrationInventory.Add([ordered]@{ name = $migration.Name; sha256 = $digest })
    $migrationLines.Add("$($migration.Name)=$digest")
}
$migrationSetSha256 = Get-Sha256ForText (($migrationLines -join "`n") + "`n")

$hostedParserSource = Get-Content -Raw -LiteralPath (Join-Path $repositoryRoot "crates/adapters/src/hosted_parser.rs")
$parserSchemaVersion = Get-Constant $hostedParserSource "PARSER_SCHEMA_VERSION" "parser schema"
$promptVersion = Get-Constant $hostedParserSource "HOSTED_PROMPT_VERSION" "parser prompt"

$parentDirectory = Split-Path -Parent $outputFullPath
if (-not (Test-Path -LiteralPath $parentDirectory)) {
    New-Item -ItemType Directory -Path $parentDirectory -Force | Out-Null
}
$candidate = [ordered]@{
    schema_version = "release-evidence-candidate-0.1.0"
    evidence_kind = "candidate"
    candidate_only = $true
    release_approved = $false
    publication_performed = $false
    production_activation_performed = $false
    deployment_performed = $false
    source = [ordered]@{
        git_commit = $gitCommit.ToLowerInvariant()
        tree_status = "clean"
        application_version = $applicationVersion
        source_build_identity = "Nutrition_backend@$($gitCommit.ToLowerInvariant())"
    }
    migrations = [ordered]@{
        count = $migrationInventory.Count
        set_sha256 = $migrationSetSha256
        files = @($migrationInventory)
    }
    parser = [ordered]@{
        schema_version = $parserSchemaVersion
        prompt_version = $promptVersion
        provider_version = $parserProviderVersion
    }
    catalog = [ordered]@{
        release_id = $parsedCatalogReleaseId.ToString()
        evidence_ref = $catalogEvidenceRef
    }
    containers = @($containerImages)
    input_document_sha256 = (Get-FileHash -LiteralPath $inputsFullPath -Algorithm SHA256).Hash.ToLowerInvariant()
    decision_boundary = "This document is candidate evidence only and cannot approve, publish, activate, deploy, or change traffic."
}
$candidate | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $outputFullPath -Encoding utf8
Write-Output "[PASS] Candidate release evidence written outside repository: $outputFullPath"
