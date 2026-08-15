$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$databaseUrl = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition"

Write-Output "Starting PostgreSQL for staged FDC importer verification..."
docker compose -f deploy/compose.yaml up -d --wait postgres

try {
    $env:TEST_DATABASE_URL = $databaseUrl
    Write-Output "Running staged FDC importer integration test..."
    cargo test -p persistence-postgres --test fdc_importer_integration -- --ignored
    Write-Output "Running explicit catalog activation integration test..."
    cargo test -p persistence-postgres --test catalog_activation_integration -- --ignored --test-threads=1
    Write-Output "Staged FDC importer verification passed."
}
finally {
    docker compose -f deploy/compose.yaml stop postgres
}
