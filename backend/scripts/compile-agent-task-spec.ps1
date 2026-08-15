param(
    [Parameter(Mandatory = $true)][string]$IntentPath,
    [Parameter(Mandatory = $true)][string]$RepositoryRoot,
    [Parameter(Mandatory = $true)][string]$BaselineCommit,
    [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = 'Stop'
$RepositoryRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$intentPath = (Resolve-Path -LiteralPath $IntentPath).Path
$intent = Get-Content -Raw -LiteralPath $intentPath | ConvertFrom-Json

function Require-Text([string]$Value, [string]$Name) {
    if ([string]::IsNullOrWhiteSpace($Value)) { throw "Task Intent $Name is required" }
}

Require-Text ([string]$intent.task_id) 'task_id'
Require-Text ([string]$intent.objective) 'objective'
if ($null -eq $intent.acceptance_criteria -or @($intent.acceptance_criteria).Count -eq 0) {
    throw 'Task Intent acceptance_criteria must be non-empty'
}
if ($null -eq $intent.non_negotiables) { throw 'Task Intent non_negotiables must be present' }
if ($null -eq $intent.approved_protected_decisions) { throw 'Task Intent approved_protected_decisions must be present' }

$commit = (& git -C $RepositoryRoot rev-parse "$BaselineCommit^{commit}" 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $BaselineCommit -notmatch '^[0-9a-fA-F]{40}$' -or $commit -notmatch '^[0-9a-fA-F]{40}$' -or $commit -ine $BaselineCommit) {
    throw 'Task Spec baseline commit must be an existing full commit SHA'
}

$scopeHints = if ($intent.PSObject.Properties.Name -contains 'scope_hints') {
    @($intent.scope_hints | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })
} else { @() }
$includes = if ($scopeHints.Count -gt 0) { $scopeHints } else { @('**') }
$excludes = if ($intent.PSObject.Properties.Name -contains 'scope_exclusions') {
    @($intent.scope_exclusions | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })
} else { @() }

$risk = if ($intent.PSObject.Properties.Name -contains 'risk_floor' -and $intent.risk_floor) {
    [string]$intent.risk_floor
} else { 'low' }
$riskLevels = @('low', 'medium', 'high', 'critical')
if ($risk -notin $riskLevels) { throw "Task Intent risk_floor is invalid: $risk" }

$riskPolicy = Get-Content -Raw -LiteralPath (Join-Path $RepositoryRoot '.agent/verification/risk-policy.json') | ConvertFrom-Json
$riskOrder = @('low', 'medium', 'high', 'critical')
foreach ($approval in @($intent.approved_protected_decisions)) {
    $domain = [string]$approval.domain
    $domainProperty = $riskPolicy.protected_domains.PSObject.Properties[$domain]
    if ($null -eq $domainProperty) { throw "Task Intent approval domain is not canonical: $domain" }
    $domainRisk = [string]$domainProperty.Value
    if ($riskOrder.IndexOf($domainRisk) -gt $riskOrder.IndexOf($risk)) { $risk = $domainRisk }
}

$approvedProtectedPaths = @(
    @($intent.approved_protected_decisions) |
        ForEach-Object { @($_.scope) } |
        Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } |
        Select-Object -Unique
)

$compiled = [ordered]@{
    schema_version = '2.0.0'
    task_id = [string]$intent.task_id
    objective = [string]$intent.objective
    acceptance_criteria = @($intent.acceptance_criteria)
    non_negotiables = @($intent.non_negotiables)
    risk_level = $risk
    scope_envelope = [ordered]@{
        include = @($includes)
        exclude = @($excludes)
        approved_protected_paths = @($approvedProtectedPaths)
    }
    approved_protected_decisions = @($intent.approved_protected_decisions)
    baseline = [ordered]@{ commit = $commit; source = 'git' }
}

$parent = Split-Path -Parent $OutputPath
New-Item -ItemType Directory -Force -Path $parent | Out-Null
$compiled | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Output "[PASS] Task Spec compiled from Task Intent: $($compiled.task_id)"
