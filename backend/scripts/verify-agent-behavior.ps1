param(
    [string]$RepositoryRoot,
    [string]$CasesPath,
    [string]$ResultsPath,
    [string]$LegacyResultsPath,
    [string]$ModernResultsPath,
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) { $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path }
else { $RepositoryRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path }

function Fail([string]$Message) { throw "[FAIL] $Message" }
function Get-RepoPath([string]$RelativePath) { Join-Path $RepositoryRoot ($RelativePath.Replace("/", "\")) }
function Has-Property($Object, [string]$Name) { $null -ne $Object.PSObject.Properties[$Name] }
function Assert-ExactProperties($Object, [string[]]$Allowed, [string]$Context) {
    $unknown = @($Object.PSObject.Properties.Name | Where-Object { $_ -notin $Allowed })
    if ($unknown.Count -gt 0) { Fail "$Context contains unknown field(s): $($unknown -join ', ')" }
}
function Load-Json([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail "JSON file does not exist: $Path" }
    try { Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json } catch { Fail "invalid JSON: $Path" }
}
function Assert-StringArray($Value, [string]$Context, [bool]$NonEmpty = $false) {
    if ($null -eq $Value -or $Value -is [string] -or $Value -isnot [System.Collections.IEnumerable]) { Fail "$Context must be an array" }
    $items = @($Value)
    if ($NonEmpty -and $items.Count -eq 0) { Fail "$Context must be non-empty" }
    if (@($items | Where-Object { $_ -isnot [string] -or [string]::IsNullOrWhiteSpace($_) }).Count -gt 0) { Fail "$Context must contain non-empty strings" }
    return $items
}
function Get-CanonicalGates {
    $map = Load-Json (Get-RepoPath ".agent/maps/verification-map.json")
    return @($map.gates | ForEach-Object { [string]$_.name })
}

function Assert-BehaviorCases($Document) {
    Assert-ExactProperties $Document @("schema_version", "scoring_fields", "cases") "behavioral eval cases"
    if ($Document.schema_version -ne "1.0.0") { Fail "behavioral eval case schema_version must be 1.0.0" }
    $expectedScoring = @("task_success", "policy_violations", "protected_decision_violations", "scope_violations", "required_gate_pass", "root_cause_success", "recovery_success", "context_relevance")
    $scoring = @(Assert-StringArray $Document.scoring_fields "behavioral scoring_fields" $true)
    $actualScoring = (@($scoring | Sort-Object) -join ",")
    $expectedScoringText = (@($expectedScoring | Sort-Object) -join ",")
    if ($actualScoring -ne $expectedScoringText) { Fail "behavioral scoring fields do not match canonical scoring model" }
    $cases = @(Assert-StringArray @($Document.cases | ForEach-Object id) "behavioral case IDs" $true)
    if ($cases.Count -lt 15) { Fail "behavioral eval suite requires at least 15 cases" }
    if (@($cases | Select-Object -Unique).Count -ne $cases.Count) { Fail "behavioral eval case IDs must be unique" }
    $expectedIds = 1..15 | ForEach-Object { "BEH-{0:D3}" -f $_ }
    foreach ($id in $expectedIds) { if ($id -notin $cases) { Fail "behavioral eval suite is missing $id" } }
    $categories = @("context_discovery", "root_cause", "scope", "invariant", "protected_decision", "recovery", "efficiency", "diff_review")
    foreach ($case in @($Document.cases)) {
        Assert-ExactProperties $case @("id", "category", "intent", "observable_requirements", "hard_failures") "behavioral eval case"
        if ([string]$case.category -notin $categories) { Fail "behavioral eval case category is invalid: $($case.id)" }
        if ([string]::IsNullOrWhiteSpace([string]$case.intent)) { Fail "behavioral eval case intent is empty: $($case.id)" }
        [void](Assert-StringArray $case.observable_requirements "behavioral observable requirements" $true)
        [void](Assert-StringArray $case.hard_failures "behavioral hard failures" $true)
    }
    return $Document
}

function Assert-BehaviorResultObject($Document, [string]$ExpectedMode) {
    Assert-ExactProperties $Document @("schema_version", "mode", "cases") "behavioral results"
    if ($Document.schema_version -ne "1.0.0" -or $Document.mode -ne $ExpectedMode) { Fail "behavioral result identity is invalid" }
    $caseIds = @((Load-Json (Get-RepoPath ".agent/evals/behavioral-cases.json")).cases | ForEach-Object id)
    $results = @($Document.cases)
    if ($results.Count -ne $caseIds.Count) { Fail "behavioral result count does not match case inventory" }
    foreach ($result in $results) {
        Assert-ExactProperties $result @("id", "task_success", "policy_violations", "protected_decision_violations", "scope_violations", "required_gate_pass", "root_cause_success", "recovery_success", "context_relevance", "evidence_refs") "behavioral result"
        if ([string]$result.id -notin $caseIds) { Fail "behavioral result references unknown case: $($result.id)" }
        foreach ($field in @("task_success", "required_gate_pass", "context_relevance")) { if ($result.$field -isnot [bool]) { Fail "behavioral result field must be boolean: $field" } }
        foreach ($field in @("policy_violations", "protected_decision_violations", "scope_violations")) { if ($result.$field -isnot [int] -or [int]$result.$field -lt 0) { Fail "behavioral result count is invalid: $field" } }
        foreach ($field in @("root_cause_success", "recovery_success")) { if ($null -ne $result.$field -and $result.$field -isnot [bool]) { Fail "behavioral result optional field is invalid: $field" } }
        [void](Assert-StringArray $result.evidence_refs "behavioral evidence_refs" $true)
        if ([int]$result.policy_violations -gt 0 -or [int]$result.protected_decision_violations -gt 0) { Fail "behavioral result has hard policy failure: $($result.id)" }
    }
    if (@($results.id | Select-Object -Unique).Count -ne $results.Count) { Fail "behavioral result IDs must be unique" }
    return $Document
}

function Compare-BehaviorResults($Legacy, $Modern) {
    [void](Assert-BehaviorResultObject $Legacy "legacy")
    [void](Assert-BehaviorResultObject $Modern "modern")
    $legacyById = @{}; $modernById = @{}
    foreach ($result in @($Legacy.cases)) { $legacyById[[string]$result.id] = $result }
    foreach ($result in @($Modern.cases)) { $modernById[[string]$result.id] = $result }
    $taskSuccessDelta = 0; $policyRegressionCount = 0; $unnecessaryBlockDelta = 0
    foreach ($id in $legacyById.Keys) {
        $legacy = $legacyById[$id]; $modern = $modernById[$id]
        if ($modern.task_success -and -not $legacy.task_success) { $taskSuccessDelta++ }
        if ([int]$modern.policy_violations -gt [int]$legacy.policy_violations -or [int]$modern.protected_decision_violations -gt [int]$legacy.protected_decision_violations) { $policyRegressionCount++ }
        if (-not $modern.task_success -and $legacy.task_success) { $unnecessaryBlockDelta-- }
    }
    [pscustomobject]@{task_success_delta=$taskSuccessDelta; policy_regression_cases=$policyRegressionCount; unnecessary_block_delta=$unnecessaryBlockDelta; case_count=$legacyById.Count}
}

function Invoke-BehaviorSelfTest {
    $cases = Assert-BehaviorCases (Load-Json (Get-RepoPath ".agent/evals/behavioral-cases.json"))
    $validCases = @($cases.cases | ForEach-Object { [pscustomobject]@{id=$_.id; task_success=$true; policy_violations=0; protected_decision_violations=0; scope_violations=0; required_gate_pass=$true; root_cause_success=$true; recovery_success=$true; context_relevance=$true; evidence_refs=@("local:behavioral-self-test") } })
    $legacy = [pscustomobject][ordered]@{schema_version="1.0.0"; mode="legacy"; cases=$validCases}
    $modern = [pscustomobject][ordered]@{schema_version="1.0.0"; mode="modern"; cases=$validCases}
    [void](Compare-BehaviorResults $legacy $modern)
    Write-Output "[PASS] Behavioral eval inventory: $(@($cases.cases).Count) cases"
    Write-Output "[PASS] Behavioral result validation and legacy/modern comparison self-test"
}

Push-Location $RepositoryRoot
try {
    if ($SelfTest) {
        Invoke-BehaviorSelfTest
        exit 0
    }
    $casesPathResolved = if ([string]::IsNullOrWhiteSpace($CasesPath)) { Get-RepoPath ".agent/evals/behavioral-cases.json" } else { $CasesPath }
    [void](Assert-BehaviorCases (Load-Json $casesPathResolved))
    if ($ResultsPath) { [void](Assert-BehaviorResultObject (Load-Json $ResultsPath) "modern") }
    if ($LegacyResultsPath -or $ModernResultsPath) {
        if (-not ($LegacyResultsPath -and $ModernResultsPath)) { Fail "comparison requires both legacy and modern result paths" }
        $comparison = Compare-BehaviorResults (Load-Json $LegacyResultsPath) (Load-Json $ModernResultsPath)
        Write-Output ($comparison | ConvertTo-Json -Compress)
    }
    Write-Output "[PASS] Behavioral eval harness validation passed."
}
catch { Write-Error $_.Exception.Message; exit 1 }
finally { Pop-Location }
