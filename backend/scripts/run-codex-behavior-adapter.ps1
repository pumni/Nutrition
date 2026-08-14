param(
    [Parameter(Mandatory = $true)][string]$RepositoryRoot,
    [Parameter(Mandatory = $true)][string]$TaskPath,
    [Parameter(Mandatory = $true)][string]$EvidencePath,
    [string]$Model = 'codex-cli-default',
    [int]$TimeoutSeconds = 600
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$RepositoryRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$TaskPath = (Resolve-Path -LiteralPath $TaskPath).Path
New-Item -ItemType Directory -Force -Path $EvidencePath | Out-Null

function Load-Json([string]$Path) { Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json }
function Write-Json([string]$Path, $Value) { $Value | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath $Path -Encoding utf8 }
function Find-Codex {
    $command = Get-Command codex.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    $candidates = @(
        (Join-Path $env:USERPROFILE '.codex/packages/standalone/current/bin/codex.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs/codex/codex.exe')
    )
    foreach ($candidate in $candidates) { if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate } }
    throw 'codex CLI executable was not found'
}

$task = Load-Json $TaskPath
$eventsPath = Join-Path $EvidencePath 'codex-events.jsonl'
$stderrPath = Join-Path $EvidencePath 'codex-stderr.log'
$finalMessagePath = Join-Path $EvidencePath 'codex-final-message.md'
$promptPath = Join-Path $EvidencePath 'codex-prompt.md'
$prompt = @"
You are running a real coding task in a disposable repository worktree.

Repository root: $RepositoryRoot
Task ID: $($task.task_id)

Repository authority:
- Read AGENTS.md and the active .agent context only as needed.
- Treat repository policy and invariants as authoritative.
- Choose the relevant context, files, implementation order, tests, and plan revisions yourself.
- A protected decision that is not approved must be reported and left unchanged.

Objective:
$($task.objective)

Acceptance criteria:
$(($task.acceptance_criteria | ForEach-Object { '- ' + $_ }) -join "`n")

Task envelope:
Include: $(($task.scope_include) -join ', ')
Exclude: $(($task.scope_exclude) -join ', ')

Execution:
1. Investigate the repository and discover the relevant context and constraints yourself.
2. Implement the smallest correct change, or stop the affected part with an evidence-based protected-decision report when approval is required.
3. Run the relevant repository tests and verification available for the changed area.
4. Inspect the final diff for unrelated changes and clean them up.
5. End with a concise report of outcome, changed paths, tests, failures/recovery, and any protected decision required.

Do not commit, push, access external systems, or write evaluator evidence into the repository.
"@
$prompt | Set-Content -LiteralPath $promptPath -Encoding utf8

$codex = Find-Codex
$arguments = @('-s', 'workspace-write', '-a', 'never', '-C', $RepositoryRoot)
if ($Model -and $Model -ne 'codex-cli-default') { $arguments += @('-m', $Model) }
$arguments += @('exec', '--ephemeral', '--json', '--color', 'never', '-o', $finalMessagePath)
$arguments += $prompt
$started = (Get-Date).ToUniversalTime().ToString('o')
$processInfo = [Diagnostics.ProcessStartInfo]::new()
$processInfo.FileName = $codex
$processInfo.UseShellExecute = $false
$processInfo.RedirectStandardOutput = $true
$processInfo.RedirectStandardError = $true
foreach ($argument in $arguments) { [void]$processInfo.ArgumentList.Add([string]$argument) }
$process = [Diagnostics.Process]::new()
$process.StartInfo = $processInfo
if (-not $process.Start()) { throw 'could not start codex CLI' }
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadToEndAsync()
$timedOut = -not $process.WaitForExit($TimeoutSeconds * 1000)
if ($timedOut) {
    try { $process.Kill($true) } catch { }
    $stdout = ''
    $stderr = 'Codex adapter timed out before completing the trial.'
} else {
    $process.WaitForExit()
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
}
$stdout | Set-Content -LiteralPath $eventsPath -Encoding utf8
$stderr | Set-Content -LiteralPath $stderrPath -Encoding utf8
if (-not (Test-Path -LiteralPath $finalMessagePath -PathType Leaf)) { Set-Content -LiteralPath $finalMessagePath -Value '' -Encoding utf8 }
$exitCode = if ($timedOut) { 124 } else { [int]$process.ExitCode }
$modelObserved = $Model
$eventLines = if (Test-Path -LiteralPath $eventsPath) { @(Get-Content -LiteralPath $eventsPath) } else { @() }
foreach ($line in $eventLines) {
    try { $event = $line | ConvertFrom-Json; if ($event.model) { $modelObserved = [string]$event.model }; if ($event.item.model) { $modelObserved = [string]$event.item.model } } catch { }
}
$result = [ordered]@{
    schema_version = '1.0.0'
    adapter = 'codex-cli'
    model = $modelObserved
    task_id = [string]$task.task_id
    exit_code = $exitCode
    task_success = ($exitCode -eq 0)
    timed_out = [bool]$timedOut
    started_at = $started
    completed_at = (Get-Date).ToUniversalTime().ToString('o')
    event_line_count = $eventLines.Count
    prompt_ref = (Split-Path -Leaf $promptPath)
    events_ref = (Split-Path -Leaf $eventsPath)
    stderr_ref = (Split-Path -Leaf $stderrPath)
    final_message_ref = (Split-Path -Leaf $finalMessagePath)
}
Write-Json (Join-Path $EvidencePath 'adapter-result.json') $result
Write-Output ($result | ConvertTo-Json -Compress)
exit $exitCode
