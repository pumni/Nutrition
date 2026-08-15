[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$TaskIntent,
    [Parameter(Mandatory = $true)][string]$RepositoryRoot,
    [Parameter(Mandatory = $true)][string]$BaselineCommit,
    [Parameter(Mandatory = $true)][string]$OutputPath
)

$ErrorActionPreference = 'Stop'
$RepositoryRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$TaskIntent = (Resolve-Path -LiteralPath $TaskIntent).Path

if ($BaselineCommit -notmatch '^[0-9a-fA-F]{40}$') {
    throw 'BaselineCommit must be a full commit SHA'
}
$resolved = (& git -C $RepositoryRoot rev-parse "$BaselineCommit^{commit}" 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or $resolved -ine $BaselineCommit) {
    throw 'BaselineCommit is not an existing commit in the trusted repository'
}

$powerShell = (Get-Command pwsh -ErrorAction SilentlyContinue)
if ($null -eq $powerShell) { $powerShell = Get-Command powershell -ErrorAction Stop }
$compiler = Join-Path $PSScriptRoot 'compile-agent-task-spec.ps1'
$arguments = @(
    '-NoLogo', '-NoProfile', '-File', $compiler,
    '-IntentPath', $TaskIntent,
    '-RepositoryRoot', $RepositoryRoot,
    '-BaselineCommit', $BaselineCommit,
    '-OutputPath', $OutputPath
)
& $powerShell.Source @arguments
if ($LASTEXITCODE -ne 0) { throw 'Task Spec preparation failed' }
Write-Output "[PASS] Task Spec prepared against baseline: $BaselineCommit"
