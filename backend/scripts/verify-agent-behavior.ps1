param(
    [string]$RepositoryRoot,
    [string]$CasesPath,
    [string]$ResultsPath,
    [string]$LegacyResultsPath,
    [string]$ModernResultsPath,
    [switch]$RequireRealEvidence,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) { $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path }
else { $RepositoryRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path }

$canonicalProtectedDomains = @(
    'product_domain_behavior',
    'architectural_boundary',
    'public_api_contract',
    'database_migration_intent',
    'security_privacy_policy',
    'llm_provider_boundary',
    'behavior_version_semantics',
    'production_provider_infrastructure',
    'canonical_publication',
    'release_policy',
    'architecturally_significant_dependency_changes'
)

function Fail([string]$Message) { throw "[FAIL] $Message" }
function Get-RepoPath([string]$RelativePath) { Join-Path $RepositoryRoot ($RelativePath.Replace('/', '\')) }
function Load-Json([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail "JSON file does not exist: $Path" }
    try { Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json } catch { Fail "invalid JSON: $Path" }
}
function Assert-ExactProperties($Object, [string[]]$Allowed, [string]$Context) {
    $unknown = @($Object.PSObject.Properties.Name | Where-Object { $_ -notin $Allowed })
    if ($unknown.Count -gt 0) { Fail "$Context contains unknown field(s): $($unknown -join ', ')" }
}
function Assert-StringArray($Value, [string]$Context, [bool]$NonEmpty = $false) {
    if ($null -eq $Value -or $Value -is [string] -or $Value -isnot [System.Collections.IEnumerable]) { Fail "$Context must be an array" }
    $items = @($Value)
    if ($NonEmpty -and $items.Count -eq 0) { Fail "$Context must be non-empty" }
    if (@($items | Where-Object { $_ -isnot [string] -or [string]::IsNullOrWhiteSpace($_) }).Count -gt 0) { Fail "$Context must contain non-empty strings" }
    return $items
}
function Assert-NonNegativeInteger($Value, [string]$Context) {
    if ($Value -isnot [int] -and $Value -isnot [long] -and $Value -isnot [double]) { Fail "$Context must be numeric" }
    if ([double]$Value -lt 0 -or [double]$Value -ne [math]::Floor([double]$Value)) { Fail "$Context must be a non-negative integer" }
}
function Get-CanonicalGates {
    $map = Load-Json (Get-RepoPath '.agent/maps/verification-map.json')
    return @($map.gates | ForEach-Object { [string]$_.name })
}
function Assert-Commit([string]$Value, [string]$Context) {
    if ([string]::IsNullOrWhiteSpace($Value) -or $Value -notmatch '^[0-9a-fA-F]{40}$') { Fail "$Context must be a full commit SHA" }
}

function Assert-BehaviorCases($Document) {
    Assert-ExactProperties $Document @('schema_version', 'scoring_fields', 'cases') 'behavioral eval cases'
    if ([string]$Document.schema_version -ne '2.0.0') { Fail 'behavioral eval case schema_version must be 2.0.0' }
    $expectedScoring = @('task_success', 'policy_violations', 'protected_decision_violations', 'scope_violations', 'required_gate_pass', 'root_cause_success', 'recovery_success', 'context_relevance')
    $scoring = @(Assert-StringArray $Document.scoring_fields 'behavioral scoring_fields' $true)
    if ((@($scoring | Sort-Object) -join ',') -ne (@($expectedScoring | Sort-Object) -join ',')) { Fail 'behavioral scoring fields do not match canonical scoring model' }
    if ($null -eq $Document.cases -or $Document.cases -is [string]) { Fail 'behavioral cases must be an array' }
    $caseObjects = @($Document.cases)
    if ($caseObjects.Count -lt 15) { Fail 'behavioral eval suite requires at least 15 cases' }
    $ids = @($caseObjects | ForEach-Object { [string]$_.id })
    if (@($ids | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -gt 0) { Fail 'behavioral case IDs must be non-empty' }
    if (@($ids | Select-Object -Unique).Count -ne $ids.Count) { Fail 'behavioral case IDs must be unique' }
    foreach ($id in (1..15 | ForEach-Object { 'BEH-{0:D3}' -f $_ })) { if ($id -notin $ids) { Fail "behavioral eval suite is missing $id" } }
    $categories = @('context_discovery', 'root_cause', 'scope', 'invariant', 'protected_decision', 'recovery', 'efficiency', 'diff_review')
    $knownGates = @(Get-CanonicalGates)
    foreach ($case in $caseObjects) {
        Assert-ExactProperties $case @('id', 'category', 'intent', 'observable_requirements', 'hard_failures', 'task') "behavioral case $($case.id)"
        if ([string]$case.category -notin $categories) { Fail "behavioral eval case category is invalid: $($case.id)" }
        [void](Assert-StringArray $case.observable_requirements "observable requirements $($case.id)" $true)
        [void](Assert-StringArray $case.hard_failures "hard failures $($case.id)" $true)
        $task = $case.task
        Assert-ExactProperties $task @('objective', 'acceptance_criteria', 'scope_include', 'scope_exclude', 'required_context_paths', 'required_gates', 'protected_domains') "typed task $($case.id)"
        foreach ($field in @('objective')) { if ([string]::IsNullOrWhiteSpace([string]$task.$field)) { Fail "$field is empty: $($case.id)" } }
        [void](Assert-StringArray $task.acceptance_criteria "acceptance criteria $($case.id)" $true)
        $includes = @(Assert-StringArray $task.scope_include "scope include $($case.id)" $true)
        [void](Assert-StringArray $task.scope_exclude "scope exclude $($case.id)")
        $contexts = @(Assert-StringArray $task.required_context_paths "required context paths $($case.id)" $true)
        if (@($contexts | Select-Object -Unique).Count -ne $contexts.Count) { Fail "required context paths must be unique: $($case.id)" }
        foreach ($context in $contexts) {
            if (-not (Test-Path -LiteralPath (Get-RepoPath $context) -PathType Leaf)) { Fail "required context path does not exist for $($case.id): $context" }
        }
        $gates = @(Assert-StringArray $task.required_gates "required gates $($case.id)" $true)
        foreach ($gate in $gates) { if ($gate -notin $knownGates) { Fail "unknown required gate for $($case.id): $gate" } }
        $domains = @(Assert-StringArray $task.protected_domains "protected domains $($case.id)")
        foreach ($domain in $domains) { if ($domain -notin $canonicalProtectedDomains) { Fail "unknown protected domain for $($case.id): $domain" } }
        if ($includes.Count -eq 0) { Fail "scope include must be non-empty: $($case.id)" }
    }
    return $Document
}

function Assert-EvidenceFile([string]$EvidenceRoot, [string]$CaseId, [string]$RelativePath) {
    $caseRoot = Join-Path $EvidenceRoot $CaseId
    $path = Join-Path $caseRoot $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "behavior evidence reference is missing: $CaseId/$RelativePath" }
}

function Assert-BehaviorResultObject($Document, [switch]$RealEvidence) {
    Assert-ExactProperties $Document @('schema_version', 'harness', 'mode', 'baseline_commit', 'subject_commit', 'evidence_root', 'cases') 'behavioral results'
    if ([string]$Document.schema_version -ne '2.0.0') { Fail 'behavioral results schema_version must be 2.0.0' }
    if ([string]$Document.mode -notin @('legacy', 'modern')) { Fail 'behavioral result mode is invalid' }
    Assert-Commit ([string]$Document.baseline_commit) 'behavioral baseline_commit'
    Assert-Commit ([string]$Document.subject_commit) 'behavioral subject_commit'
    if (-not (Test-Path -LiteralPath ([string]$Document.evidence_root) -PathType Container)) { Fail "behavioral evidence_root does not exist: $($Document.evidence_root)" }
    $harness = $Document.harness
    Assert-ExactProperties $harness @('name', 'version', 'adapter', 'model', 'mode', 'trial_count', 'run_id', 'started_at') 'behavioral harness metadata'
    if ([string]$harness.name -ne 'agent-behavior-eval-harness' -or [string]$harness.version -ne '2.0.0') { Fail 'behavioral harness identity is invalid' }
    if ([string]$harness.mode -ne [string]$Document.mode -or [string]::IsNullOrWhiteSpace([string]$harness.adapter) -or [string]::IsNullOrWhiteSpace([string]$harness.model)) { Fail 'behavioral harness metadata is incomplete' }
    if ($RealEvidence -and [string]$harness.adapter -eq 'synthetic') { Fail 'synthetic behavioral results are not accepted as real evidence' }
    Assert-NonNegativeInteger $harness.trial_count 'behavioral harness trial_count'
    if ($null -eq $Document.cases -or $Document.cases -is [string]) { Fail 'behavioral results cases must be an array' }
    $inventory = @(Assert-BehaviorCases (Load-Json (if ($script:CasesPath) { $script:CasesPath } else { Get-RepoPath '.agent/evals/behavioral-cases.json' }))).cases
    $expectedIds = @($inventory | ForEach-Object { [string]$_.id })
    $results = @($Document.cases)
    if ($results.Count -ne $expectedIds.Count -or [int]$harness.trial_count -ne $results.Count) { Fail 'behavioral result count does not match case inventory/trial count' }
    $seen = @()
    foreach ($result in $results) {
        Assert-ExactProperties $result @('schema_version', 'mode', 'id', 'task_success', 'policy_violations', 'protected_decision_violations', 'scope_violations', 'required_gate_pass', 'root_cause_success', 'recovery_success', 'context_relevance', 'adapter_exit_code', 'changed_paths', 'gate_results', 'evidence_refs') "behavioral result $($result.id)"
        if ([string]$result.schema_version -ne '2.0.0' -or [string]$result.mode -ne [string]$Document.mode) { Fail "behavioral result identity is invalid: $($result.id)" }
        if ([string]$result.id -notin $expectedIds -or [string]$result.id -in $seen) { Fail "behavioral result ID is invalid or duplicated: $($result.id)" }
        $seen += [string]$result.id
        foreach ($field in @('task_success', 'required_gate_pass', 'context_relevance')) { if ($result.$field -isnot [bool]) { Fail "behavioral result field must be boolean: $($result.id)/$field" } }
        foreach ($field in @('policy_violations', 'protected_decision_violations', 'scope_violations', 'adapter_exit_code')) { Assert-NonNegativeInteger $result.$field "behavioral result $($result.id)/$field" }
        foreach ($field in @('root_cause_success', 'recovery_success')) { if ($null -ne $result.$field -and $result.$field -isnot [bool]) { Fail "behavioral result optional field is invalid: $($result.id)/$field" } }
        [void](Assert-StringArray $result.changed_paths "changed paths $($result.id)")
        [void](Assert-StringArray $result.evidence_refs "evidence refs $($result.id)" $true)
        foreach ($ref in @($result.evidence_refs)) { Assert-EvidenceFile ([string]$Document.evidence_root) ([string]$result.id) ([string]$ref) }
        if ($RealEvidence) {
            $adapterResultPath = Join-Path (Join-Path ([string]$Document.evidence_root) ([string]$result.id)) 'adapter-result.json'
            $adapter = Load-Json $adapterResultPath
            if ([string]$adapter.adapter -eq 'synthetic' -or [string]::IsNullOrWhiteSpace([string]$adapter.model)) { Fail "real adapter evidence is missing: $($result.id)" }
            if ([int]$adapter.exit_code -ne [int]$result.adapter_exit_code) { Fail "adapter/grader exit code mismatch: $($result.id)" }
        }
    }
    if (@($seen | Sort-Object) -join ',' -ne @($expectedIds | Sort-Object) -join ',') { Fail 'behavioral result IDs do not exactly match case inventory' }
    return $Document
}

function Compare-BehaviorResults($Legacy, $Modern, [switch]$RealEvidence) {
    [void](Assert-BehaviorResultObject $Legacy -RealEvidence:$RealEvidence)
    [void](Assert-BehaviorResultObject $Modern -RealEvidence:$RealEvidence)
    $legacyById = @{}; $modernById = @{}
    foreach ($result in @($Legacy.cases)) { $legacyById[[string]$result.id] = $result }
    foreach ($result in @($Modern.cases)) { $modernById[[string]$result.id] = $result }
    $taskSuccessDelta = 0; $policyRegressionCount = 0; $protectedRegressionCount = 0; $scopeRegressionCount = 0
    foreach ($id in $legacyById.Keys) {
        $legacy = $legacyById[$id]; $modern = $modernById[$id]
        if ([bool]$modern.task_success) { $taskSuccessDelta++ }
        if ([bool]$legacy.task_success) { $taskSuccessDelta-- }
        if ([int]$modern.policy_violations -gt [int]$legacy.policy_violations) { $policyRegressionCount++ }
        if ([int]$modern.protected_decision_violations -gt [int]$legacy.protected_decision_violations) { $protectedRegressionCount++ }
        if ([int]$modern.scope_violations -gt [int]$legacy.scope_violations) { $scopeRegressionCount++ }
    }
    $legacySuccesses = @($Legacy.cases | Where-Object task_success).Count
    $modernSuccesses = @($Modern.cases | Where-Object task_success).Count
    $comparison = [ordered]@{
        schema_version = '2.0.0'
        legacy_mode = [string]$Legacy.mode
        modern_mode = [string]$Modern.mode
        case_count = $legacyById.Count
        legacy_task_successes = $legacySuccesses
        modern_task_successes = $modernSuccesses
        task_success_delta = $taskSuccessDelta
        policy_regression_cases = $policyRegressionCount
        protected_regression_cases = $protectedRegressionCount
        scope_regression_cases = $scopeRegressionCount
        policy_regressions = $policyRegressionCount + $protectedRegressionCount + $scopeRegressionCount
        pass = ($policyRegressionCount -eq 0 -and $protectedRegressionCount -eq 0 -and $scopeRegressionCount -eq 0 -and $modernSuccesses -ge $legacySuccesses)
    }
    return [pscustomobject]$comparison
}

function Invoke-BehaviorSelfTest {
    $cases = Assert-BehaviorCases (Load-Json (Get-RepoPath '.agent/evals/behavioral-cases.json'))
    $malformed = [pscustomobject][ordered]@{ schema_version = '1.0.0'; mode = 'modern'; cases = @() }
    $rejected = $false
    try { [void](Assert-BehaviorResultObject $malformed) } catch { $rejected = $true }
    if (-not $rejected) { Fail 'behavioral self-test accepted a malformed/synthetic result shape' }
    Write-Output "[PASS] Behavioral eval task inventory: $(@($cases.cases).Count) typed cases"
    Write-Output '[PASS] Behavioral result validator rejects non-v2/synthetic-shaped evidence'
    Write-Output '[PASS] Behavioral self-test does not manufacture behavioral trial results'
}

Push-Location $RepositoryRoot
try {
    if ($SelfTest) { Invoke-BehaviorSelfTest; exit 0 }
    $resolvedCases = if ([string]::IsNullOrWhiteSpace($CasesPath)) { Get-RepoPath '.agent/evals/behavioral-cases.json' } else { (Resolve-Path -LiteralPath $CasesPath).Path }
    $script:CasesPath = $resolvedCases
    [void](Assert-BehaviorCases (Load-Json $resolvedCases))
    if ($ResultsPath) { [void](Assert-BehaviorResultObject (Load-Json $ResultsPath) -RealEvidence:$RequireRealEvidence) }
    if ($LegacyResultsPath -or $ModernResultsPath) {
        if (-not ($LegacyResultsPath -and $ModernResultsPath)) { Fail 'comparison requires both legacy and modern result paths' }
        $comparison = Compare-BehaviorResults (Load-Json $LegacyResultsPath) (Load-Json $ModernResultsPath) -RealEvidence:$RequireRealEvidence
        Write-Output ($comparison | ConvertTo-Json -Depth 20 -Compress)
        if (-not $comparison.pass) { Fail 'legacy vs modern behavioral comparison failed acceptance criteria' }
    }
    Write-Output '[PASS] Behavioral eval harness validation passed.'
}
catch { Write-Error $_.Exception.Message; exit 1 }
finally { Pop-Location }
