$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

Write-Output "Validating agent context layer..."
& "$PSScriptRoot\verify-agent-context.ps1"

Write-Output "Checking Rust formatting..."
cargo fmt --all -- --check

Write-Output "Running Clippy..."
cargo clippy --workspace --all-targets -- -D warnings

Write-Output "Running workspace tests..."
cargo test --workspace

Write-Output "Validating JSON artifacts..."
Get-Content -Raw -LiteralPath ".\schemas\parsed-meal-0.1.0.json" |
    ConvertFrom-Json |
    Out-Null

Get-Content -Raw -LiteralPath ".\fixtures\vietnamese-meal-bench\manifest.json" |
    ConvertFrom-Json |
    Out-Null

Get-Content -Raw -LiteralPath ".\fixtures\vietnamese-meal-bench\foundation-cases.json" |
    ConvertFrom-Json |
    Out-Null

Write-Output "Checking prohibited sensitive logging patterns..."
$previousNativeErrorPreference = $PSNativeCommandUseErrorActionPreference
$PSNativeCommandUseErrorActionPreference = $false
$sensitiveLogMatches = rg --line-number `
    '(info|warn|error|debug|trace)!\([^)]*(request\.text|raw_text|authorization|database_url)' `
    crates
$sensitiveLogExitCode = $LASTEXITCODE
$PSNativeCommandUseErrorActionPreference = $previousNativeErrorPreference
if ($sensitiveLogExitCode -eq 0) {
    throw "Potential sensitive value found in a logging macro: $sensitiveLogMatches"
}
if ($sensitiveLogExitCode -gt 1) {
    throw "Sensitive logging scan failed"
}

Write-Output "Validating Docker Compose configuration..."
docker compose -f deploy/compose.yaml config --quiet

Write-Output "Foundation verification passed."
