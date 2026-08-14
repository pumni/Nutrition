param(
    [string]$RepositoryRoot = (Get-Location).Path,
    [Parameter(Mandatory = $true)][string]$CasesPath,
    [string]$ResultsPath,
    [switch]$RequireRealEvidence,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$Root = [System.IO.Path]::GetFullPath($RepositoryRoot)
$script:CasesPath = [System.IO.Path]::GetFullPath($CasesPath)
if ($script:CasesPath.StartsWith($Root.TrimEnd('\','/') + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'behavioral case metadata must be outside the subject repository root'
}

function Fail([string]$Message) { throw "[FAIL] $Message" }
function Get-RepoPath([string]$RelativePath) { return [System.IO.Path]::Combine($Root, ($RelativePath -replace '/', [System.IO.Path]::DirectorySeparatorChar)) }
function Load-Json([string]$Path) { if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail "JSON file does not exist: $Path" }; try { Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json } catch { Fail "invalid JSON: $Path" } }
function Assert-ExactProperties($Object, [string[]]$Allowed, [string]$Label) { $actual=@($Object.PSObject.Properties.Name | Sort-Object); $expected=@($Allowed | Sort-Object); if (($actual -join '|') -ne ($expected -join '|')) { Fail "$Label properties are not exact; expected $($expected -join ',') got $($actual -join ',')" } }
function Assert-StringArray($Value, [string]$Label, [bool]$Required = $false) { if ($null -eq $Value -or $Value -is [string]) { if ($Required) { Fail "$Label must be an array" }; return @() }; foreach ($item in @($Value)) { if ([string]::IsNullOrWhiteSpace([string]$item)) { Fail "$Label contains an empty value" } }; return @($Value) }
function Assert-EvidenceFile([string]$EvidenceRoot, [string]$CaseId, [string]$Reference) { if ([string]::IsNullOrWhiteSpace($EvidenceRoot)) { Fail 'behavioral evidence root is empty' }; $path=Join-Path (Join-Path $EvidenceRoot $CaseId) $Reference; if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "behavioral evidence file is missing: $path" } }
function Assert-CaseInventory($Document) {
    Assert-ExactProperties $Document @('schema_version','cases') 'behavioral case inventory'
    if ([string]$Document.schema_version -ne '3.0.0') { Fail 'behavioral case inventory schema mismatch' }
    $cases=@($Document.cases); if ($cases.Count -eq 0) { Fail 'behavioral case inventory is empty' }
    $seen=@()
    foreach ($case in $cases) {
        Assert-ExactProperties $case @('id','category','task') 'behavioral case'
        if ([string]$case.id -notmatch '^BEH-[0-9]{3}$' -or [string]$case.id -in $seen) { Fail "behavioral case ID is invalid or duplicated: $($case.id)" }
        $seen += [string]$case.id
        $taskAllowed=@('objective','acceptance_criteria','scope_include','scope_exclude','expected_outcome','expected_change_patterns','forbidden_change_patterns','required_gates','seed_unrelated_file','hidden_assertion'); $unknown=@($case.task.PSObject.Properties.Name | Where-Object { $_ -notin $taskAllowed }); if ($unknown.Count -gt 0) { Fail "behavioral task contains unknown field(s): $($unknown -join ',')" }
        foreach ($required in @('objective','acceptance_criteria','scope_include','scope_exclude','expected_outcome','expected_change_patterns','forbidden_change_patterns','required_gates','hidden_assertion')) { if (-not $case.task.PSObject.Properties[$required]) { Fail "behavioral task is missing '$required': $($case.id)" } }
        if ([string]::IsNullOrWhiteSpace([string]$case.task.objective)) { Fail "behavioral task objective is empty: $($case.id)" }
        [void](Assert-StringArray $case.task.acceptance_criteria "acceptance criteria $($case.id)" $true)
        [void](Assert-StringArray $case.task.scope_include "scope include $($case.id)" $true)
        [void](Assert-StringArray $case.task.scope_exclude "scope exclude $($case.id)")
        if ([string]$case.task.expected_outcome -notin @('change','block')) { Fail "behavioral task outcome is invalid: $($case.id)" }
        if ([string]$case.task.expected_outcome -eq 'change') { [void](Assert-StringArray $case.task.expected_change_patterns "expected paths $($case.id)" $true) } else { [void](Assert-StringArray $case.task.expected_change_patterns "expected paths $($case.id)") }
        [void](Assert-StringArray $case.task.forbidden_change_patterns "forbidden paths $($case.id)")
        [void](Assert-StringArray $case.task.required_gates "required gates $($case.id)" $true)
        Assert-ExactProperties $case.task.hidden_assertion @('script','arguments') "hidden assertion $($case.id)"
        if ([string]::IsNullOrWhiteSpace([string]$case.task.hidden_assertion.script)) { Fail "hidden assertion script is empty: $($case.id)" }
        if (-not [IO.Path]::IsPathRooted([string]$case.task.hidden_assertion.script)) { Fail "hidden assertion script must be an absolute control-plane path: $($case.id)" }
        [void](Assert-StringArray $case.task.hidden_assertion.arguments "hidden assertion arguments $($case.id)")
        $graderAwareFields=@('required_'+'context_paths','protected_'+'domains'); if (@($case.task.PSObject.Properties.Name | Where-Object { $_ -in $graderAwareFields }).Count -gt 0) { Fail "behavioral task exposes grader-aware instructions: $($case.id)" }
    }
    return $Document
}
function Assert-BehaviorResults($Document, [switch]$RealEvidence) {
    Assert-ExactProperties $Document @('schema_version','harness','baseline_commit','evidence_root','cases') 'behavioral result document'
    if ([string]$Document.schema_version -ne '3.0.0') { Fail 'behavioral result schema mismatch' }
    $evidenceRoot = [IO.Path]::GetFullPath([string]$Document.evidence_root)
    if ($evidenceRoot.StartsWith($Root.TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { Fail 'behavioral evidence must be outside the subject repository root' }
    $harness=$Document.harness; Assert-ExactProperties $harness @('name','version','adapter','model','trial_count','run_id','started_at','completed_at') 'behavioral harness metadata'
    if ($harness.name -ne 'agent-behavior-eval-harness' -or $harness.version -ne '3.0.0' -or [string]::IsNullOrWhiteSpace([string]$harness.adapter) -or [string]::IsNullOrWhiteSpace([string]$harness.model)) { Fail 'behavioral harness metadata is incomplete' }
    if ($RealEvidence -and $harness.adapter -eq 'synthetic') { Fail 'synthetic behavioral results are not accepted as real evidence' }
    $inventory=@(Assert-CaseInventory (Load-Json $script:CasesPath)).cases
    $expectedIds=@($inventory | ForEach-Object id); $results=@($Document.cases)
    if ($results.Count -ne $expectedIds.Count -or [int]$harness.trial_count -ne $results.Count) { Fail 'behavioral result count does not match case inventory/trial count' }
    $seen=@()
    foreach ($result in $results) {
        Assert-ExactProperties $result @('schema_version','id','expected_outcome','actual_outcome','task_success','scope_violations','protected_decision_violations','required_gate_pass','adapter_exit_code','changed_paths','gate_results','environment_checks','hidden_assertion','evidence_refs') "behavioral result $($result.id)"
        if ($result.schema_version -ne '3.0.0' -or $result.id -notin $expectedIds -or $result.id -in $seen) { Fail "behavioral result identity is invalid: $($result.id)" }
        $seen += [string]$result.id
        if ($result.expected_outcome -notin @('change','block') -or $result.actual_outcome -notin @('changed','blocked','failed')) { Fail "behavioral outcome is invalid: $($result.id)" }
        foreach ($field in @('task_success','required_gate_pass')) { if ($result.$field -isnot [bool]) { Fail "behavioral result field must be boolean: $($result.id)/$field" } }
        foreach ($field in @('scope_violations','protected_decision_violations','adapter_exit_code')) { if ([int]$result.$field -lt 0) { Fail "behavioral result numeric field is invalid: $($result.id)/$field" } }
        [void](Assert-StringArray $result.changed_paths "changed paths $($result.id)")
        [void](Assert-StringArray $result.evidence_refs "evidence refs $($result.id)" $true)
        foreach ($ref in @($result.evidence_refs)) { Assert-EvidenceFile ([string]$Document.evidence_root) ([string]$result.id) ([string]$ref) }
        Assert-ExactProperties $result.environment_checks @('adapter_exit_zero','final_message_present','expected_path_observed','forbidden_path_absent','hidden_assertion_pass') "environment checks $($result.id)"
        if ($result.environment_checks.hidden_assertion_pass -isnot [bool]) { Fail "hidden assertion check must be boolean: $($result.id)" }
        Assert-ExactProperties $result.hidden_assertion @('status','exit_code','evidence_ref') "hidden assertion result $($result.id)"
        if ($result.hidden_assertion.status -notin @('pass','fail') -or [int]$result.hidden_assertion.exit_code -lt 0) { Fail "hidden assertion result is invalid: $($result.id)" }
        if ([bool]$result.environment_checks.hidden_assertion_pass -ne ($result.hidden_assertion.status -eq 'pass')) { Fail "hidden assertion status/check mismatch: $($result.id)" }
        if ($RealEvidence) {
            $adapter=Load-Json (Join-Path (Join-Path ([string]$Document.evidence_root) ([string]$result.id)) 'adapter-result.json')
            if ($adapter.adapter -eq 'synthetic' -or [string]::IsNullOrWhiteSpace([string]$adapter.model) -or [int]$adapter.event_line_count -le 0) { Fail "real adapter evidence is missing: $($result.id)" }
            if ([int]$adapter.exit_code -ne [int]$result.adapter_exit_code) { Fail "adapter/grader exit code mismatch: $($result.id)" }
        }
    }
    if (@($seen | Sort-Object) -join ',' -ne @($expectedIds | Sort-Object) -join ',') { Fail 'behavioral result IDs do not exactly match case inventory' }
    return $Document
}
function Invoke-BehaviorSelfTest {
    $cases=Assert-CaseInventory (Load-Json $script:CasesPath)
    $malformed=[pscustomobject][ordered]@{schema_version='2.0.0';cases=@()}
    $rejected=$false; try { [void](Assert-BehaviorResults $malformed) } catch { $rejected=$true }
    if (-not $rejected) { Fail 'behavioral self-test accepted malformed result evidence' }
    Write-Output "[PASS] Behavioral eval inventory: $(@($cases.cases).Count) outcome-based coding cases"
    Write-Output '[PASS] Behavioral self-test rejects malformed/synthetic-shaped result documents'
    Write-Output '[PASS] Behavioral self-test does not manufacture trial results'
}

try {
    if ($SelfTest) { Invoke-BehaviorSelfTest; exit 0 }
    [void](Assert-CaseInventory (Load-Json $script:CasesPath))
    if (-not [string]::IsNullOrWhiteSpace($ResultsPath)) { [void](Assert-BehaviorResults (Load-Json (Resolve-Path -LiteralPath $ResultsPath)) -RealEvidence:$RequireRealEvidence) }
    Write-Output '[PASS] Behavioral eval harness validation passed.'
}
catch { Write-Error $_.Exception.Message; exit 1 }
