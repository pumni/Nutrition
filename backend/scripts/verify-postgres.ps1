$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$databaseUrl = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition"

Write-Output "Starting PostgreSQL 18..."
docker compose -f deploy/compose.yaml up -d postgres

try {
    $env:DATABASE_URL = $databaseUrl
    $env:RUN_MIGRATIONS = "true"
    $env:RUN_FOUNDATION_SEED = "true"

    Write-Output "Applying migrations and the test-only foundation seed..."
    cargo run -p worker

    $env:TEST_DATABASE_URL = $databaseUrl
    Write-Output "Running PostgreSQL integration tests..."
    cargo test -p persistence-postgres --test postgres_integration -- --ignored

    Write-Output "Building and smoke-testing the PostgreSQL-backed API..."
    cargo build -p api-http
    $apiPath = (Resolve-Path ".\target\debug\api-http.exe").Path
    $apiProcess = Start-Process -FilePath $apiPath -PassThru -WindowStyle Hidden
    try {
        $ready = $false
        for ($attempt = 0; $attempt -lt 20; $attempt++) {
            try {
                $readiness = Invoke-RestMethod `
                    -Uri "http://127.0.0.1:8080/health/ready" `
                    -TimeoutSec 1
                $ready = $readiness.status -eq "ready"
                if ($ready) {
                    break
                }
            }
            catch {
                Start-Sleep -Milliseconds 250
            }
        }
        if (-not $ready) {
            throw "PostgreSQL-backed API did not become ready"
        }

        $requestBody = @{
            text = "2 quả trứng gà luộc, 1 bát cơm trắng"
            locale = "vi-VN"
            mode = "balanced"
        } | ConvertTo-Json
        $created = Invoke-RestMethod `
            -Method Post `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses" `
            -ContentType "application/json; charset=utf-8" `
            -Body ([Text.Encoding]::UTF8.GetBytes($requestBody)) `
            -TimeoutSec 5
        $replayed = Invoke-RestMethod `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses/$($created.analysis_id)" `
            -TimeoutSec 5

        if (
            $created.status -ne "completed" -or
            $created.analysis_id -ne $replayed.analysis_id -or
            $created.revision_id -ne $replayed.revision_id -or
            $replayed.calculation.totals.Count -ne 4 -or
            $replayed.items[0].lower_mass_g -ne 90 -or
            $replayed.items[0].upper_mass_g -ne 120 -or
            $replayed.items[1].lower_mass_g -ne 120 -or
            $replayed.items[1].upper_mass_g -ne 200
        ) {
            throw "HTTP create/read replay contract failed"
        }
    }
    finally {
        if ($apiProcess -and -not $apiProcess.HasExited) {
            Stop-Process -Id $apiProcess.Id
        }
    }

    Write-Output "Running database immutability verification..."
    docker compose -f deploy/compose.yaml exec -T postgres `
        psql -U nutrition -d nutrition -f /migrations/tests/immutability.sql

    Write-Output "PostgreSQL verification passed."
}
finally {
    docker compose -f deploy/compose.yaml stop postgres
}
