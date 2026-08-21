[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$InputPath,
    [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$repositoryPrefix = $repositoryRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar

function Assert-ExternalPath {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Label)
    $fullPath = [IO.Path]::GetFullPath($Path)
    if ($fullPath.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label must be outside the repository"
    }
    return $fullPath
}

function Assert-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Value, [Parameter(Mandatory = $true)][string]$Label)
    if ($Value -notmatch '^sha256:[0-9a-fA-F]{64}$') {
        throw "$Label must be a full sha256 digest"
    }
}

function Assert-SafeReference {
    param([Parameter(Mandatory = $true)][string]$Value, [Parameter(Mandatory = $true)][string]$Label)
    if ($Value -notmatch '^[A-Za-z0-9:/._-]{1,256}$') {
        throw "$Label is not a safe external reference"
    }
}

function Get-MigrationInventorySha256 {
    $migrationDirectory = Join-Path $repositoryRoot "migrations"
    $lines = @(
        Get-ChildItem -LiteralPath $migrationDirectory -File -Filter "*.sql" |
            Sort-Object Name |
            ForEach-Object {
                $digest = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                "$($_.Name)=$digest"
            }
    )
    if ($lines.Count -eq 0) {
        throw "no migration files found for rollback compatibility binding"
    }
    $text = ($lines -join "`n") + "`n"
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($text))).Replace('-', '')).ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

$inputFullPath = Assert-ExternalPath -Path $InputPath -Label "InputPath"
$outputFullPath = Assert-ExternalPath -Path $OutputPath -Label "OutputPath"
if (-not (Test-Path -LiteralPath $inputFullPath -PathType Leaf)) {
    throw "rollback evidence input does not exist"
}

$raw = Get-Content -Raw -LiteralPath $inputFullPath
try {
    $input = $raw | ConvertFrom-Json
}
catch {
    throw "rollback evidence input is not valid JSON"
}

$required = @(
    "schema_version",
    "environment",
    "production_authorization",
    "current_application_digest",
    "previous_application_digest",
    "current_config_fingerprint",
    "previous_config_fingerprint",
    "migration_compatibility",
    "catalog_rollback_validation",
    "catalog_source_status",
    "catalog_activation_performed",
    "rollback_plan_sha256",
    "migration_inventory_sha256",
    "catalog_manifest_path",
    "catalog_manifest_sha256",
    "catalog_membership_count",
    "artifact_evidence_ref"
)
foreach ($name in $required) {
    if ($null -eq $input.PSObject.Properties[$name]) {
        throw "rollback evidence input is missing '$name'"
    }
}
$allowed = @($required | Sort-Object)
$actual = @($input.PSObject.Properties.Name | Sort-Object)
if ((ConvertTo-Json $actual -Compress) -ne (ConvertTo-Json $allowed -Compress)) {
    throw "rollback evidence input contains unknown fields"
}
if ([string]$input.schema_version -ne "recovery-rollback-evidence-0.1.0" -or
    [string]$input.environment -notin @("synthetic-local", "staging") -or
    [bool]$input.production_authorization -or
    [bool]$input.catalog_activation_performed) {
    throw "rollback evidence input is not a non-production synthetic validation"
}
foreach ($name in @(
        "current_application_digest",
        "previous_application_digest",
        "current_config_fingerprint",
        "previous_config_fingerprint"
    )) {
    Assert-Sha256 -Value ([string]$input.$name) -Label $name
}
if ([string]$input.current_application_digest -eq [string]$input.previous_application_digest -or
    [string]$input.current_config_fingerprint -eq [string]$input.previous_config_fingerprint) {
    throw "rollback target must differ from the current application/configuration"
}
Assert-Sha256 -Value ([string]$input.rollback_plan_sha256) -Label "rollback_plan_sha256"
Assert-Sha256 -Value ([string]$input.migration_inventory_sha256) -Label "migration_inventory_sha256"
Assert-Sha256 -Value ([string]$input.catalog_manifest_sha256) -Label "catalog_manifest_sha256"
Assert-SafeReference -Value ([string]$input.artifact_evidence_ref) -Label "artifact_evidence_ref"
if ([int64]$input.catalog_membership_count -le 0) {
    throw "catalog_membership_count must be positive"
}
if ([string]$input.migration_compatibility -ne "verified-forward-only" -or
    [string]$input.catalog_rollback_validation -ne "verified-staged-immutable-snapshot" -or
    [string]$input.catalog_source_status -ne "superseded") {
    throw "rollback evidence input does not satisfy forward-only/catalog rollback gates"
}

$rollbackPlanPath = Join-Path (Join-Path $repositoryRoot "deploy") (Join-Path "recovery" "rollback-plan.json")
$actualPlanSha = (Get-FileHash -LiteralPath $rollbackPlanPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ([string]$input.rollback_plan_sha256 -ne "sha256:$actualPlanSha") {
    throw "rollback evidence is not bound to the current rollback plan"
}
$actualMigrationInventorySha = Get-MigrationInventorySha256
if ([string]$input.migration_inventory_sha256 -ne "sha256:$actualMigrationInventorySha") {
    throw "rollback evidence is not bound to the current migration inventory"
}
$catalogManifestPath = Assert-ExternalPath -Path ([string]$input.catalog_manifest_path) -Label "catalog_manifest_path"
if (-not (Test-Path -LiteralPath $catalogManifestPath -PathType Leaf)) {
    throw "catalog rollback manifest does not exist outside the repository"
}
$actualCatalogManifestSha = (Get-FileHash -LiteralPath $catalogManifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ([string]$input.catalog_manifest_sha256 -ne "sha256:$actualCatalogManifestSha") {
    throw "catalog rollback manifest SHA-256 does not match the input binding"
}
try {
    $catalogManifest = Get-Content -Raw -LiteralPath $catalogManifestPath | ConvertFrom-Json
}
catch {
    throw "catalog rollback manifest is not valid JSON"
}
$catalogRequired = @(
    "schema_version",
    "environment",
    "source_status",
    "validation_status",
    "membership_count",
    "manifest_checksum_sha256",
    "activation_performed"
)
foreach ($name in $catalogRequired) {
    if ($null -eq $catalogManifest.PSObject.Properties[$name]) {
        throw "catalog rollback manifest is missing '$name'"
    }
}
$catalogActualFields = @($catalogManifest.PSObject.Properties.Name | Sort-Object)
$catalogExpectedFields = @($catalogRequired | Sort-Object)
if ((ConvertTo-Json $catalogActualFields -Compress) -ne (ConvertTo-Json $catalogExpectedFields -Compress)) {
    throw "catalog rollback manifest contains unknown fields"
}
if ([string]$catalogManifest.schema_version -ne "catalog-rollback-manifest-0.1.0" -or
    [string]$catalogManifest.environment -notin @("synthetic-local", "staging") -or
    [string]$catalogManifest.source_status -ne "superseded" -or
    [string]$catalogManifest.validation_status -ne "verified" -or
    [bool]$catalogManifest.activation_performed) {
    throw "catalog rollback manifest is not a verified inactive superseded-release record"
}
Assert-Sha256 -Value ([string]$catalogManifest.manifest_checksum_sha256) -Label "catalog manifest checksum"
if ([int64]$catalogManifest.membership_count -ne [int64]$input.catalog_membership_count) {
    throw "catalog membership count does not match the rollback evidence input"
}

$evidence = [ordered]@{
    schema_version = "recovery-rollback-validation-0.2.0"
    status = "passed"
    evidence_kind = "synthetic-local-rollback-validation"
    production_authorization = $false
    deployment_performed = $false
    catalog_activation_performed = $false
    application = [ordered]@{
        current_application_digest = [string]$input.current_application_digest
        previous_application_digest = [string]$input.previous_application_digest
        current_config_fingerprint = [string]$input.current_config_fingerprint
        previous_config_fingerprint = [string]$input.previous_config_fingerprint
        immutable_target_required = $true
    }
    migrations = [ordered]@{
        compatibility = "verified-forward-only"
        applied_migrations_mutated = $false
        migration_inventory_sha256 = [string]$input.migration_inventory_sha256
    }
    catalog = [ordered]@{
        source_status = "superseded"
        validation = "verified-staged-immutable-snapshot"
        activation_performed = $false
        manifest_sha256 = [string]$input.catalog_manifest_sha256
        membership_count = [int64]$input.catalog_membership_count
    }
    rollback_plan_sha256 = [string]$input.rollback_plan_sha256
    decision_boundary = "This validates the rollback evidence shape and gates only; it does not deploy, activate, change traffic, or authorize production."
    input_sha256 = (Get-FileHash -LiteralPath $inputFullPath -Algorithm SHA256).Hash.ToLowerInvariant()
    generated_at_utc = [DateTimeOffset]::UtcNow.ToString("O")
}
$parent = Split-Path -Parent $outputFullPath
if (-not (Test-Path -LiteralPath $parent)) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
}
$evidence | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath $outputFullPath -Encoding utf8
Write-Output "[PASS] Rollback validation evidence written outside repository: $OutputPath"
