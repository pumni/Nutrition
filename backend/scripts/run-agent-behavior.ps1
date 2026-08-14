param(
    [string]$RepositoryRoot,
    [string]$CasesPath,
    [Parameter(Mandatory = $true)][ValidateSet('legacy', 'modern')][string]$Mode,
    [Parameter(Mandatory = $true)][string]$AdapterScript,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [string]$BaselineCommit,
    [string[]]$CaseIds
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
function Normalize([string]$Path) { $Path.Replace('\', '/').TrimStart('./') }

$casesDocument = Load-Json $CasesPath
$cases = @($casesDocument.cases)
if ($CaseIds -and $CaseIds.Count -gt 0) { $cases = @($cases | Where-Object { [string]$_.id -in $CaseIds }) }
if ($cases.Count -eq 0) { Fail 'no behavioral cases selected' }
if (-not $CaseIds -and $cases.Count -lt 15) { Fail 'behavioral case inventory is incomplete' }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$runId = '{0}-{1}-{2}' -f $Mode, (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ'), ([guid]::NewGuid().ToString('N').Substring(0, 8))
$evidenceRoot = Join-Path $OutputDirectory $runId
New-Item -ItemType Directory -Force -Path $evidenceRoot | Out-Null
$adapterModel = if ([string]::IsNullOrWhiteSpace($env:CODEX_MODEL)) { 'codex-cli-default' } else { [string]$env:CODEX_MODEL }
$caseResults = @()
$worktreeRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('nutrition-agent-eval-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $worktreeRoot | Out-Null

try {
    foreach ($case in $cases) {
        $caseId = [string]$case.id
        $caseEvidence = Join-Path $evidenceRoot $caseId
        New-Item -ItemType Directory -Force -Path $caseEvidence | Out-Null
        $worktree = Join-Path $worktreeRoot $caseId
        $taskSpecPath = Join-Path $worktreeRoot ("{0}-task-spec.json" -f $caseId)
        $adapterResultPath = Join-Path $caseEvidence 'adapter-result.json'
        $adapterInvocationLog = Join-Path $caseEvidence 'adapter-invocation.log'
        $taskSpec = [ordered]@{
            schema_version = '2.0.0'
            task_id = $caseId
            mode = $Mode
            objective = [string]$case.task.objective
            acceptance_criteria = @($case.task.acceptance_criteria)
            scope_include = @($case.task.scope_include)
            scope_exclude = @($case.task.scope_exclude)
            required_context_paths = @($case.task.required_context_paths)
            required_gates = @($case.task.required_gates)
            protected_domains = @($case.task.protected_domains)
        }
        Write-Json $taskSpecPath $taskSpec
        $created = $false
        try {
            Invoke-Git @('worktree', 'add', '--detach', $worktree, $BaselineCommit) | Out-Null
            $created = $true
            $adapterArgs = @{
                RepositoryRoot = $worktree
                TaskSpecPath = $taskSpecPath
                CasePath = $CasesPath
                Mode = $Mode
                EvidencePath = $caseEvidence
                Model = $adapterModel
            }
            & $AdapterScript @adapterArgs *> $adapterInvocationLog
            $adapterExit = if ($null -eq $LASTEXITCODE) { 0 } else { [int]$LASTEXITCODE }
            if (-not (Test-Path -LiteralPath $adapterResultPath -PathType Leaf)) {
                Write-Json $adapterResultPath ([ordered]@{
                    schema_version = '1.0.0'
                    adapter = 'unknown'
                    model = $adapterModel
                    mode = $Mode
                    task_id = $caseId
                    exit_code = $adapterExit
                    task_success = $false
                    error = 'adapter did not produce adapter-result.json'
                })
            }
            $gradeScript = Join-Path $RepositoryRoot 'scripts/grade-agent-behavior.ps1'
            & $gradeScript -RepositoryRoot $worktree -CasePath $CasesPath -BaselineCommit $BaselineCommit -AdapterResultPath $adapterResultPath -EvidenceDirectory $caseEvidence -Mode $Mode *> (Join-Path $caseEvidence 'grader.log')
            $gradeExit = if ($null -eq $LASTEXITCODE) { 0 } else { [int]$LASTEXITCODE }
            $resultPath = Join-Path $caseEvidence 'result.json'
            if ($gradeExit -ne 0 -or -not (Test-Path -LiteralPath $resultPath -PathType Leaf)) {
                Fail "grader failed for $caseId; inspect $(Join-Path $caseEvidence 'grader.log')"
            }
            $result = Load-Json $resultPath
            $caseResults += $result
            Copy-Item -LiteralPath $resultPath -Destination (Join-Path $OutputDirectory ("{0}.json" -f $caseId)) -Force
        }
        finally {
            if ($created) {
                try { Invoke-Git @('worktree', 'remove', '--force', $worktree) | Out-Null } catch { $_ | Out-String | Set-Content -LiteralPath (Join-Path $caseEvidence 'worktree-cleanup-error.log') -Encoding utf8 }
            }
            if (Test-Path -LiteralPath $taskSpecPath) { Remove-Item -LiteralPath $taskSpecPath -Force }
        }
    }
}
finally {
    if (Test-Path -LiteralPath $worktreeRoot) { Remove-Item -LiteralPath $worktreeRoot -Recurse -Force }
}

$aggregate = [ordered]@{
    schema_version = '2.0.0'
    harness = [ordered]@{
        name = 'agent-behavior-eval-harness'
        version = '2.0.0'
        adapter = 'codex-cli'
        model = $adapterModel
        mode = $Mode
        trial_count = $caseResults.Count
        run_id = $runId
        started_at = (Get-Date).ToUniversalTime().ToString('o')
    }
    mode = $Mode
    baseline_commit = $BaselineCommit
    subject_commit = (& git -C $RepositoryRoot rev-parse HEAD 2>&1 | Out-String).Trim()
    evidence_root = (Resolve-Path -LiteralPath $evidenceRoot).Path
    cases = @($caseResults)
}
Write-Json (Join-Path $OutputDirectory 'results.json') $aggregate
Write-Output ($aggregate | ConvertTo-Json -Depth 40 -Compress)
