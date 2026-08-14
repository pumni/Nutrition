param(
    [Parameter(Mandatory = $true)][string]$IntentPath,
    [Parameter(Mandatory = $true)][string]$RepositoryRoot,
    [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = 'Stop'
$RepositoryRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$intent = Get-Content -Raw -LiteralPath (Resolve-Path -LiteralPath $IntentPath).Path | ConvertFrom-Json
$router = Get-Content -Raw -LiteralPath (Join-Path $RepositoryRoot '.agent/context/router.json') | ConvertFrom-Json
$commit = (& git -C $RepositoryRoot rev-parse HEAD 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw 'could not resolve Task Spec baseline commit' }

function Test-Glob([string]$Path, [string]$Pattern) {
    $escaped = [regex]::Escape($Pattern.Replace('\', '/')).Replace('\*\*', '.*').Replace('\*', '[^/]*').Replace('\?', '[^/]')
    return [regex]::IsMatch($Path.Replace('\', '/'), "^$escaped$", [Text.RegularExpressions.RegexOptions]::IgnoreCase)
}

$scopeHints = if ($intent.PSObject.Properties.Name -contains 'scope_hints') { @($intent.scope_hints | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }) } else { @() }
$includes = if ($scopeHints.Count -gt 0) { $scopeHints } else { @('**') }
$excludes = if ($intent.PSObject.Properties.Name -contains 'scope_exclusions') { @($intent.scope_exclusions | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }) } else { @() }
$modules = @($router.default_modules)
$gates = @()
foreach ($route in @($router.path_routes)) {
    if (@($includes | Where-Object { Test-Glob $_ ([string]$route.path_pattern) }).Count -gt 0) {
        $modules += @($route.modules)
        $gates += @($route.mandatory_gates)
    }
}
if ($gates.Count -eq 0) { $gates += 'acl-integrity' }
$modules = @($modules | Select-Object -Unique)
$gates = @($gates | Select-Object -Unique)
$domains = @($intent.approved_protected_decisions | ForEach-Object { [string]$_.domain } | Select-Object -Unique)
$risk = if ($intent.risk_floor) { [string]$intent.risk_floor } else { 'low' }
$riskPolicy = Get-Content -Raw -LiteralPath (Join-Path $RepositoryRoot '.agent/verification/risk-policy.json') | ConvertFrom-Json
$riskOrder = @('low', 'medium', 'high', 'critical')
foreach ($domain in $domains) {
    $domainRisk = [string]$riskPolicy.domain_defaults.$domain.risk_level
    if ($riskOrder.IndexOf($domainRisk) -gt $riskOrder.IndexOf($risk)) { $risk = $domainRisk }
}
$approvedProtectedPaths = @($intent.approved_protected_decisions | ForEach-Object { $_.scope } | ForEach-Object { $_ } | Select-Object -Unique)
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
    protected_boundaries = $domains
    required_policy_modules = $modules
    required_verification_gates = $gates
    approved_protected_decisions = @($intent.approved_protected_decisions)
    baseline = [ordered]@{commit = $commit; source = 'git'}
}
$parent = Split-Path -Parent $OutputPath
New-Item -ItemType Directory -Force -Path $parent | Out-Null
$compiled | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Output "[PASS] Task Spec compiled: $($compiled.task_id)"
