[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$CandidateEvidencePath,
    [Parameter(Mandatory = $true)][string]$GateInputsPath,
    [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
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

function Read-Json {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Label)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Label does not exist"
    }
    $raw = Get-Content -Raw -LiteralPath $Path
    if ($raw -match '(?i)(password|secret|token|api[_-]?key|authorization|bearer|private[_-]?key|database[_-]?url)\s*[:=]') {
        throw "$Label contains a prohibited secret-bearing field"
    }
    try {
        return [pscustomobject]@{ raw = $raw; value = ($raw | ConvertFrom-Json) }
    }
    catch {
        throw "$Label is not valid JSON"
    }
}

function Assert-ExactProperties {
    param(
        [Parameter(Mandatory = $true)][object]$Object,
        [Parameter(Mandatory = $true)][string[]]$Allowed,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $actual = @($Object.PSObject.Properties.Name | Sort-Object)
    $expected = @($Allowed | Sort-Object)
    if ((ConvertTo-Json $actual -Compress) -ne (ConvertTo-Json $expected -Compress)) {
        throw "$Label contains unknown or missing fields"
    }
}

function Require-Text {
    param([Parameter(Mandatory = $true)][object]$Object, [Parameter(Mandatory = $true)][string]$Name, [Parameter(Mandatory = $true)][string]$Label)
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property -or $null -eq $property.Value -or [string]::IsNullOrWhiteSpace([string]$property.Value)) {
        throw "$Label is missing '$Name'"
    }
    return [string]$property.Value
}

function Assert-Boolean {
    param(
        [Parameter(Mandatory = $true)][object]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][bool]$Expected,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property -or $property.Value -isnot [bool] -or [bool]$property.Value -ne $Expected) {
        throw "$Label must contain Boolean '$Name'=$Expected"
    }
}

function Assert-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Value, [Parameter(Mandatory = $true)][string]$Label)
    if ($Value -notmatch '^sha256:[0-9a-fA-F]{64}$') {
        throw "$Label must be a full sha256 digest"
    }
}

function Assert-SafeReference {
    param([Parameter(Mandatory = $true)][string]$Value, [Parameter(Mandatory = $true)][string]$Label)
    if ($Value -notmatch '^[A-Za-z0-9:/._@+-]{1,512}$' -or
        $Value -match '(?i)://[^/\s:]+:[^@\s]+@' -or
        $Value -match '(?i)(?:^|[^a-z])(sk|ghp|xoxb|bearer)[-_][a-z0-9_-]{8,}') {
        throw "$Label is not a safe evidence reference"
    }
}

function Assert-SafeText {
    param([Parameter(Mandatory = $true)][string]$Value, [Parameter(Mandatory = $true)][string]$Label)
    if ($Value.Length -gt 1024 -or
        $Value -match '(?i)://[^/\s:]+:[^@\s]+@' -or
        $Value -match '(?i)-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----' -or
        $Value -match '(?i)(?:^|[^a-z])(sk|ghp|xoxb|bearer)[-_][a-z0-9_-]{8,}') {
        throw "$Label contains unsafe evidence text"
    }
}

function Get-GitValue {
    param([Parameter(Mandatory = $true)][string[]]$Arguments, [Parameter(Mandatory = $true)][string]$Label)
    $value = (& git -C $repositoryRoot -c core.excludesFile= @Arguments 2>$null | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($value)) {
        throw "could not resolve $Label"
    }
    return $value
}

$candidatePath = Assert-ExternalPath -Path $CandidateEvidencePath -Label "CandidateEvidencePath"
$gateInputPath = Assert-ExternalPath -Path $GateInputsPath -Label "GateInputsPath"
$outputFullPath = Assert-ExternalPath -Path $OutputPath -Label "OutputPath"
$candidateDocument = Read-Json -Path $candidatePath -Label "candidate evidence"
$gateDocument = Read-Json -Path $gateInputPath -Label "staging gate input"
$candidate = $candidateDocument.value
$gateInput = $gateDocument.value

Assert-ExactProperties -Object $candidate -Allowed @(
    "schema_version", "evidence_kind", "candidate_only", "release_approved", "publication_performed",
    "production_activation_performed", "deployment_performed", "source", "migrations", "parser", "catalog",
    "containers", "input_document_sha256", "decision_boundary"
) -Label "candidate evidence"
if ([string]$candidate.schema_version -ne "release-evidence-candidate-0.1.0" -or
    [string]$candidate.evidence_kind -ne "candidate" -or
    [string]$candidate.source.tree_status -ne "clean") {
    throw "candidate evidence is not a non-published candidate-only document"
}
Assert-Boolean -Object $candidate -Name "candidate_only" -Expected $true -Label "candidate evidence"
Assert-Boolean -Object $candidate -Name "release_approved" -Expected $false -Label "candidate evidence"
Assert-Boolean -Object $candidate -Name "publication_performed" -Expected $false -Label "candidate evidence"
Assert-Boolean -Object $candidate -Name "production_activation_performed" -Expected $false -Label "candidate evidence"
Assert-Boolean -Object $candidate -Name "deployment_performed" -Expected $false -Label "candidate evidence"

Assert-ExactProperties -Object $candidate.source -Allowed @("git_commit", "tree_status", "application_version", "source_build_identity") -Label "candidate source"
Assert-ExactProperties -Object $candidate.migrations -Allowed @("count", "set_sha256", "files") -Label "candidate migrations"
Assert-ExactProperties -Object $candidate.parser -Allowed @("schema_version", "prompt_version", "provider_version") -Label "candidate parser"
Assert-ExactProperties -Object $candidate.catalog -Allowed @("release_id", "evidence_ref") -Label "candidate catalog"
Assert-Sha256 -Value ("sha256:" + [string]$candidate.input_document_sha256) -Label "candidate input document SHA-256"
Assert-Sha256 -Value ("sha256:" + [string]$candidate.migrations.set_sha256) -Label "candidate migration set SHA-256"
$candidateCatalogId = [Guid]::Empty
if (-not [Guid]::TryParse([string]$candidate.catalog.release_id, [ref]$candidateCatalogId)) {
    throw "candidate catalog release ID is not an unambiguous UUID"
}
Assert-SafeReference -Value ([string]$candidate.catalog.evidence_ref) -Label "candidate catalog evidence reference"
Assert-SafeReference -Value ([string]$candidate.parser.provider_version) -Label "candidate parser provider version"
if ([int64]$candidate.migrations.count -le 0 -or @($candidate.migrations.files).Count -ne [int64]$candidate.migrations.count) {
    throw "candidate migration inventory is incomplete"
}
foreach ($migration in @($candidate.migrations.files)) {
    Assert-ExactProperties -Object $migration -Allowed @("name", "sha256") -Label "candidate migration entry"
    Assert-Sha256 -Value ("sha256:" + [string]$migration.sha256) -Label "candidate migration entry SHA-256"
}
if (@($candidate.containers).Count -eq 0) {
    throw "candidate has no container image digests"
}
foreach ($image in @($candidate.containers)) {
    Assert-ExactProperties -Object $image -Allowed @("name", "reference", "digest") -Label "candidate container image"
    Assert-SafeReference -Value (Require-Text $image "name" "candidate container image") -Label "candidate container image name"
    Assert-SafeReference -Value (Require-Text $image "reference" "candidate container image") -Label "candidate container image reference"
    Assert-Sha256 -Value ([string]$image.digest) -Label "candidate container image digest"
}

Assert-ExactProperties -Object $gateInput -Allowed @(
    "schema_version", "environment", "candidate_evidence_sha256", "auth_config_fingerprint",
    "auth_config_artifact_path", "auth_config_evidence_ref", "behavior_vector",
    "rollback_target_artifact_path", "rollback_target_evidence_sha256", "rollback_target_evidence_ref", "gates"
) -Label "staging gate input"
if ([string]$gateInput.schema_version -ne "staging-release-gate-input-0.1.0" -or
    [string]$gateInput.environment -notin @("synthetic-local", "staging")) {
    throw "staging gate input has an unsupported schema or environment"
}
Assert-Sha256 -Value ([string]$gateInput.candidate_evidence_sha256) -Label "candidate evidence binding"
$actualCandidateSha = (Get-FileHash -LiteralPath $candidatePath -Algorithm SHA256).Hash.ToLowerInvariant()
if ([string]$gateInput.candidate_evidence_sha256 -ine "sha256:$actualCandidateSha") {
    throw "staging gate input is not bound to the candidate evidence document"
}
Assert-Sha256 -Value ([string]$gateInput.auth_config_fingerprint) -Label "auth configuration fingerprint"
$authArtifactPath = Assert-ExternalPath -Path ([string]$gateInput.auth_config_artifact_path) -Label "auth configuration artifact path"
if (-not (Test-Path -LiteralPath $authArtifactPath -PathType Leaf)) {
    throw "auth configuration artifact does not exist"
}
$actualAuthArtifactSha = (Get-FileHash -LiteralPath $authArtifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ([string]$gateInput.auth_config_fingerprint -ine "sha256:$actualAuthArtifactSha") {
    throw "auth configuration fingerprint does not match its external artifact"
}
Assert-SafeReference -Value ([string]$gateInput.auth_config_evidence_ref) -Label "auth configuration evidence reference"
Assert-SafeReference -Value ([string]$gateInput.behavior_vector) -Label "behavior vector"
$rollbackTargetArtifactPath = Assert-ExternalPath -Path ([string]$gateInput.rollback_target_artifact_path) -Label "rollback target artifact path"
if (-not (Test-Path -LiteralPath $rollbackTargetArtifactPath -PathType Leaf)) {
    throw "rollback target evidence artifact does not exist"
}
Assert-Sha256 -Value ([string]$gateInput.rollback_target_evidence_sha256) -Label "rollback target evidence SHA-256"
$actualRollbackTargetSha = (Get-FileHash -LiteralPath $rollbackTargetArtifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ([string]$gateInput.rollback_target_evidence_sha256 -ine "sha256:$actualRollbackTargetSha") {
    throw "rollback target evidence SHA-256 does not match its external artifact"
}
Assert-SafeReference -Value ([string]$gateInput.rollback_target_evidence_ref) -Label "rollback target evidence reference"

$gitCommit = Get-GitValue @("rev-parse", "--verify", "HEAD") "Git commit"
$gitStatus = (& git -C $repositoryRoot -c core.excludesFile= status --porcelain --untracked-files=all 2>$null | Out-String).Trim()
if (-not [string]::IsNullOrWhiteSpace($gitStatus)) {
    throw "working tree is not clean; staging gate evidence must bind to a clean source commit"
}

$requiredGateIds = @(
    "M0-governance",
    "M1-provider-privacy",
    "M2-vietnamese-benchmark",
    "M3-catalog-production-eligibility",
    "M4-staging-slo-load-restore",
    "M5-release-rollback"
)
if (@($gateInput.gates).Count -ne $requiredGateIds.Count) {
    throw "staging gate input must contain exactly the six M0-M5 gates"
}
$gateRecords = [Collections.Generic.List[object]]::new()
foreach ($gate in @($gateInput.gates)) {
    Assert-ExactProperties -Object $gate -Allowed @("id", "status", "evidence_ref", "artifact_path", "artifact_sha256", "waiver_ref") -Label "staging gate record"
    if ([string]$gate.id -notin $requiredGateIds -or @($gateInput.gates | Where-Object id -eq $gate.id).Count -ne 1) {
        throw "staging gate IDs must be the unique M0-M5 set"
    }
    if ([string]$gate.status -notin @("pass", "blocked", "waived")) {
        throw "staging gate '$($gate.id)' has an unsupported status"
    }
    Assert-SafeReference -Value ([string]$gate.evidence_ref) -Label "staging gate evidence reference"
    if ([string]$gate.status -eq "pass") {
        $artifactPath = Assert-ExternalPath -Path ([string]$gate.artifact_path) -Label "staging gate artifact path"
        if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
            throw "passed staging gate '$($gate.id)' artifact does not exist"
        }
        $actualArtifactSha = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
        Assert-Sha256 -Value ([string]$gate.artifact_sha256) -Label "staging gate artifact SHA-256"
        if ([string]$gate.artifact_sha256 -ine "sha256:$actualArtifactSha") {
            throw "passed staging gate '$($gate.id)' artifact SHA-256 does not match"
        }
        if ($null -ne $gate.waiver_ref) { throw "passed staging gate '$($gate.id)' cannot carry a waiver" }
        $artifactDocument = Read-Json -Path $artifactPath -Label "passed staging gate artifact"
        Assert-ExactProperties -Object $artifactDocument.value -Allowed @(
            "schema_version", "gate_id", "subject_commit", "candidate_evidence_sha256",
            "result", "evidence_ref", "production_authorization", "scope", "rationale", "waiver_ref"
        ) -Label "passed staging gate artifact"
        if ([string]$artifactDocument.value.schema_version -ne "staging-gate-evidence-wrapper-0.1.0" -or
            [string]$artifactDocument.value.gate_id -ne [string]$gate.id -or
            [string]$artifactDocument.value.subject_commit -ine $gitCommit -or
            [string]$artifactDocument.value.candidate_evidence_sha256 -ine [string]$gateInput.candidate_evidence_sha256 -or
            [string]$artifactDocument.value.result -ne "pass" -or
            [string]$artifactDocument.value.evidence_ref -ne [string]$gate.evidence_ref -or
            [string]$artifactDocument.value.scope -ne "staging-only" -or
            $null -ne $artifactDocument.value.waiver_ref) {
            throw "passed staging gate '$($gate.id)' artifact wrapper is not bound to this candidate and gate"
        }
        Assert-Boolean -Object $artifactDocument.value -Name "production_authorization" -Expected $false -Label "passed staging gate '$($gate.id)' artifact wrapper"
        Assert-SafeText -Value ([string]$artifactDocument.value.rationale) -Label "passed staging gate rationale"
    }
    elseif ([string]$gate.status -eq "blocked") {
        if ($null -ne $gate.artifact_path -or $null -ne $gate.artifact_sha256 -or $null -ne $gate.waiver_ref) {
            throw "blocked staging gate '$($gate.id)' cannot claim an artifact or waiver"
        }
    }
    else {
        throw "waived staging gate '$($gate.id)' requires an explicitly reviewed waiver; this gate does not accept waived records"
    }
    $gateRecords.Add([ordered]@{
            id = [string]$gate.id
            status = [string]$gate.status
            evidence_ref = [string]$gate.evidence_ref
            artifact_sha256 = if ($null -eq $gate.artifact_sha256) { $null } else { ([string]$gate.artifact_sha256).ToLowerInvariant() }
            waiver_ref = if ($null -eq $gate.waiver_ref) { $null } else { [string]$gate.waiver_ref }
        })
}
foreach ($requiredGateId in $requiredGateIds) {
    if (@($gateRecords | Where-Object id -eq $requiredGateId).Count -ne 1) {
        throw "staging gate '$requiredGateId' is missing"
    }
}

if ([string]$candidate.source.git_commit -ine $gitCommit) {
    throw "candidate evidence commit does not match the current source commit"
}
$rollbackPlanPath = Join-Path (Join-Path $repositoryRoot "deploy") (Join-Path "recovery" "rollback-plan.json")
if (-not (Test-Path -LiteralPath $rollbackPlanPath -PathType Leaf)) {
    throw "rollback plan is missing"
}
$rollbackPlanSha = (Get-FileHash -LiteralPath $rollbackPlanPath -Algorithm SHA256).Hash.ToLowerInvariant()
$applicationVersion = [string]$candidate.source.application_version
$behaviorVector = "application:$applicationVersion/parser:$($candidate.parser.prompt_version)/schema:$($candidate.parser.schema_version)/provider:$($candidate.parser.provider_version)"
if ($behaviorVector -ne [string]$gateInput.behavior_vector) {
    throw "behavior vector does not match the candidate parser/application identity"
}
$blocking = @($gateRecords | Where-Object { $_.status -eq "blocked" } | ForEach-Object id)
$allClosed = $blocking.Count -eq 0
$parentDirectory = Split-Path -Parent $outputFullPath
if (-not (Test-Path -LiteralPath $parentDirectory)) {
    New-Item -ItemType Directory -Path $parentDirectory -Force | Out-Null
}
$evidence = [ordered]@{
    schema_version = "staging-release-gate-evidence-0.1.0"
    evidence_kind = "staging-release-candidate-gate"
    status = if ($allClosed) { "ready_for_owner_release_review" } else { "blocked" }
    candidate_only = $true
    release_approved = $false
    publication_performed = $false
    production_activation_performed = $false
    deployment_performed = $false
    source = [ordered]@{
        git_commit = $gitCommit.ToLowerInvariant()
        tree_status = "clean"
        application_version = $applicationVersion
        candidate_evidence_sha256 = "sha256:$actualCandidateSha"
    }
    behavior_vector = $behaviorVector
    auth = [ordered]@{
        environment = [string]$gateInput.environment
        configuration_fingerprint = [string]$gateInput.auth_config_fingerprint
        evidence_ref = [string]$gateInput.auth_config_evidence_ref
        artifact_sha256 = [string]$gateInput.auth_config_fingerprint
    }
    parser = [ordered]@{
        schema_version = [string]$candidate.parser.schema_version
        prompt_version = [string]$candidate.parser.prompt_version
        provider_version = [string]$candidate.parser.provider_version
    }
    catalog = [ordered]@{
        release_id = $candidateCatalogId.ToString()
        evidence_ref = [string]$candidate.catalog.evidence_ref
    }
    migrations = [ordered]@{
        count = [int64]$candidate.migrations.count
        set_sha256 = [string]$candidate.migrations.set_sha256
    }
    containers = @($candidate.containers | ForEach-Object { [ordered]@{ name = [string]$_.name; reference = [string]$_.reference; digest = [string]$_.digest } })
    gates = @($gateRecords | Sort-Object id)
    rollback = [ordered]@{
        target_evidence_ref = [string]$gateInput.rollback_target_evidence_ref
        target_artifact_sha256 = [string]$gateInput.rollback_target_evidence_sha256
        rollback_plan_sha256 = "sha256:$rollbackPlanSha"
        production_authorization = $false
    }
    blockers = @($blocking | Sort-Object)
    decision_boundary = "Candidate evidence only. This document cannot approve, publish, activate, deploy, or change traffic; production release remains an external owner-controlled gate."
    generated_at_utc = [DateTimeOffset]::UtcNow.ToString("O")
}
$evidence | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $outputFullPath -Encoding utf8
Write-Output "[PASS] Staging release gate evidence written outside repository: $OutputPath"
if (-not $allClosed) {
    Write-Output "[BLOCKED] Candidate remains blocked by: $($blocking -join ', ')"
}
