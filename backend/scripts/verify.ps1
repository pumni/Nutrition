$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

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

Write-Output "Validating Docker Compose configuration..."
docker compose -f deploy/compose.yaml config --quiet

Write-Output "Foundation verification passed."
