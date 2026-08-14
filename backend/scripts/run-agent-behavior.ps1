param(
    [string]$RepositoryRoot,
    [string]$CasesPath,
    [Parameter(Mandatory = $true)][string]$AdapterScript,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [string]$BaselineCommit,
    [string[]]$CaseIds,
    [int]$AdapterTimeoutSeconds = 600
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) { $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path }
else { $RepositoryRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path }
if ([string]::IsNullOrWhiteSpace($CasesPath)) { $CasesPath = Join-Path $RepositoryRoot '.agent/evals/behavioral-cases.json' }
else { $CasesPath = (Resolve-Path -LiteralPath $CasesPath).Path }
$AdapterScript = (Resolve-Path -LiteralPath $AdapterScript).Path
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
if ([string]::IsNullOrWhiteSpace($BaselineCommit)) {
    $BaselineCommit = (& git -C $RepositoryRoot rev-parse HEAD 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw "could not resolve baseline commit: $BaselineCommit" }
}

function Fail([string]$Message) { throw "[FAIL] $Message" }
function Load-Json([string]$Path) { try { Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json } catch { Fail "invalid JSON: $Path" } }
function Write-Json([string]$Path, $Value) {
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $Value | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath $Path -Encoding utf8
}
function Invoke-Git([string[]]$Arguments) {
    $output = & git -C $RepositoryRoot @Arguments 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { Fail "git command failed: git $($Arguments -join ' ')`n$output" }
    return $output.Trim()
}

$casesDocument = Load-Json $CasesPath
$cases = @($casesDocument.cases)
if ($CaseIds -and $CaseIds.Count -gt 0) { $cases = @($cases | Where-Object { [string]$_.id -in $CaseIds }) }
if ($cases.Count -eq 0) { Fail 'no behavioral cases selected' }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$runId = 'behavior-{0}-{1}' -f (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ'), ([guid]::NewGuid().ToString('N').Substring(0, 8))
$evidenceRoot = Join-Path $OutputDirectory $runId
New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
$adapterModel = if ([string]::IsNullOrWhiteSpace($env:CODEX_MODEL)) { 'codex-cli-default' } else { [string]$env:CODEX_MODEL }
$startedAt = (Get-Date).ToUniversalTime().ToString('o')
$caseResults = @()
$worktreeRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('nutrition-agent-eval-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $worktreeRoot | Out-Null

try {
    foreach ($case in $cases) {
        $caseId = [string]$case.id
        $caseEvidence = Join-Path $evidenceRoot $caseId
        New-Item -ItemType Directory -Force -Path $caseEvidence | Out-Null
        $worktree = Join-Path $worktreeRoot $caseId
        $taskPath = Join-Path $worktreeRoot ("{0}-task.json" -f $caseId)
        $adapterResultPath = Join-Path $caseEvidence 'adapter-result.json'
        $adapterInvocationLog = Join-Path $caseEvidence 'adapter-invocation.log'
        $task = [ordered]@{
            schema_version = '3.0.0'
            task_id = $caseId
            objective = [string]$case.task.objective
            acceptance_criteria = @($case.task.acceptance_criteria)
            scope_include = @($case.task.scope_include)
            scope_exclude = @($case.task.scope_exclude)
        }
        Write-Json $taskPath $task
        $created = $false
        try {
            Invoke-Git @('worktree', 'add', '--detach', $worktree, $BaselineCommit) | Out-Null
            $created = $true
            if ($case.task.seed_unrelated_file) {
                $seedPath = Join-Path $worktree ([string]$case.task.seed_unrelated_file)
                New-Item -ItemType Directory -Force -Path (Split-Path -Parent $seedPath) | Out-Null
                Set-Content -LiteralPath $seedPath -Value 'evaluation artifact; remove before completion' -Encoding utf8
            }
            $adapterArgs = @{
                RepositoryRoot = $worktree
                TaskPath = $taskPath
                EvidencePath = $caseEvidence
                Model = $adapterModel
                TimeoutSeconds = $AdapterTimeoutSeconds
            }
            $nativePreference = $PSNativeCommandUseErrorActionPreference
            $PSNativeCommandUseErrorActionPreference = $false
            try { & $AdapterScript @adapterArgs *> $adapterInvocationLog } finally { $PSNativeCommandUseErrorActionPreference = $nativePreference }
            $adapterExit = if ($null -eq $LASTEXITCODE) { 0 } else { [int]$LASTEXITCODE }
            if (-not (Test-Path -LiteralPath $adapterResultPath -PathType Leaf)) {
                Write-Json $adapterResultPath ([ordered]@{
                    schema_version = '1.0.0'
                    adapter = 'unknown'
                    model = $adapterModel
                    task_id = $caseId
                    exit_code = $adapterExit
                    task_success = $false
                    error = 'adapter did not produce adapter-result.json'
                })
            }
            $gradeScript = Join-Path $RepositoryRoot 'scripts/grade-agent-behavior.ps1'
            & $gradeScript -RepositoryRoot $worktree -CasePath $CasesPath -BaselineCommit $BaselineCommit -AdapterResultPath $adapterResultPath -EvidenceDirectory $caseEvidence *> (Join-Path $caseEvidence 'grader.log')
            $gradeExit = if ($null -eq $LASTEXITCODE) { 0 } else { [int]$LASTEXITCODE }
            $resultPath = Join-Path $caseEvidence 'result.json'
            if ($gradeExit -ne 0 -or -not (Test-Path -LiteralPath $resultPath -PathType Leaf)) { Fail "grader failed for $caseId; inspect $(Join-Path $caseEvidence 'grader.log')" }
            $result = Load-Json $resultPath
            $caseResults += $result
            Copy-Item -LiteralPath $resultPath -Destination (Join-Path $OutputDirectory ("{0}.json" -f $caseId)) -Force
        }
        finally {
            if ($created) { try { Invoke-Git @('worktree', 'remove', '--force', $worktree) | Out-Null } catch { $_ | Out-String | Set-Content -LiteralPath (Join-Path $caseEvidence 'worktree-cleanup-error.log') -Encoding utf8 } }
            if (Test-Path -LiteralPath $taskPath) { Remove-Item -LiteralPath $taskPath -Force }
        }
    }
}
finally {
    if (Test-Path -LiteralPath $worktreeRoot) { Remove-Item -LiteralPath $worktreeRoot -Recurse -Force }
}

$aggregate = [ordered]@{
    schema_version = '3.0.0'
    harness = [ordered]@{
        name = 'agent-behavior-eval-harness'
        version = '3.0.0'
        adapter = 'codex-cli'
        model = $adapterModel
        trial_count = $caseResults.Count
        run_id = $runId
        started_at = $startedAt
        completed_at = (Get-Date).ToUniversalTime().ToString('o')
    }
    baseline_commit = $BaselineCommit
    evidence_root = (Resolve-Path -LiteralPath $evidenceRoot).Path
    cases = @($caseResults)
}
Write-Json (Join-Path $OutputDirectory 'results.json') $aggregate
Write-Output ($aggregate | ConvertTo-Json -Depth 40 -Compress)
