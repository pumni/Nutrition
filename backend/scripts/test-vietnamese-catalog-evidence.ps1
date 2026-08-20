[CmdletBinding()]
param([string]$RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path)

$ErrorActionPreference = "Stop"
$validator = Join-Path (Resolve-Path -LiteralPath $RepositoryRoot).Path "scripts/validate-vietnamese-catalog-evidence.ps1"
& pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass -File $validator -RepositoryRoot $RepositoryRoot -SelfTest
if ($LASTEXITCODE -ne 0) { throw "Vietnamese catalog evidence validator self-test failed" }
Write-Output "[PASS] Vietnamese catalog evidence regression test completed."
