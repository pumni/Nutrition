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

    Write-Output "Running one bounded worker batch..."
    $env:RUN_MIGRATIONS = "false"
    $env:RUN_FOUNDATION_SEED = "false"
    $env:WORKER_MODE = "run-once"
    $env:WORKER_ID = "verification-worker"
    cargo run -p worker
    Remove-Item Env:\WORKER_MODE

    Write-Output "Building and smoke-testing the PostgreSQL-backed API..."
    $env:AUTH_MODE = "development"
    $env:PARSER_MODE = "fixture"
    cargo build -p api-http
    $apiBinaryName = if ($IsWindows) { "api-http.exe" } else { "api-http" }
    $apiPath = (Resolve-Path (Join-Path ".\target\debug" $apiBinaryName)).Path
    $processArguments = @{
        FilePath = $apiPath
        PassThru = $true
    }
    if ($IsWindows) {
        $processArguments.WindowStyle = "Hidden"
    }
    $apiProcess = Start-Process @processArguments
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

        $verificationRunId = [guid]::NewGuid().ToString("N")
        $createKey = "foundation-create-04-$verificationRunId"
        $correctionKey = "foundation-correction-04-$verificationRunId"
        $authorization = "Bearer dev:0198f100-0000-7000-8000-000000000098"
        $createHeaders = @{
            Authorization = $authorization
            "Idempotency-Key" = $createKey
        }
        $correctionHeaders = @{
            Authorization = $authorization
            "Idempotency-Key" = $correctionKey
        }
        $authHeaders = @{Authorization = $authorization}
        $requestBody = @{
            text = "2 quả trứng gà luộc, 1 bát cơm trắng"
            locale = "vi-VN"
            mode = "balanced"
        } | ConvertTo-Json
        $created = Invoke-RestMethod `
            -Method Post `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses" `
            -Headers $createHeaders `
            -ContentType "application/json; charset=utf-8" `
            -Body ([Text.Encoding]::UTF8.GetBytes($requestBody)) `
            -TimeoutSec 5
        $idempotentCreateReplay = Invoke-RestMethod `
            -Method Post `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses" `
            -Headers $createHeaders `
            -ContentType "application/json; charset=utf-8" `
            -Body ([Text.Encoding]::UTF8.GetBytes($requestBody)) `
            -TimeoutSec 5
        $replayed = Invoke-RestMethod `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses/$($created.analysis_id)" `
            -Headers $authHeaders `
            -TimeoutSec 5

        if (
            $created.status -ne "completed" -or
            $created.revision_id -ne $idempotentCreateReplay.revision_id -or
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
        $unauthorizedRead = Invoke-WebRequest `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses/$($created.analysis_id)" `
            -SkipHttpErrorCheck `
            -TimeoutSec 5
        $foreignRead = Invoke-WebRequest `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses/$($created.analysis_id)" `
            -Headers @{Authorization = "Bearer dev:0198f100-0000-7000-8000-000000000097"} `
            -SkipHttpErrorCheck `
            -TimeoutSec 5
        if ($unauthorizedRead.StatusCode -ne 401 -or $foreignRead.StatusCode -ne 403) {
            throw "HTTP authentication/ownership contract failed"
        }

        $conflictingBody = @{
            text = "100 g trứng gà luộc"
            locale = "vi-VN"
            mode = "balanced"
        } | ConvertTo-Json
        $idempotencyConflict = Invoke-WebRequest `
            -Method Post `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses" `
            -Headers $createHeaders `
            -ContentType "application/json; charset=utf-8" `
            -Body ([Text.Encoding]::UTF8.GetBytes($conflictingBody)) `
            -SkipHttpErrorCheck `
            -TimeoutSec 5
        if ($idempotencyConflict.StatusCode -ne 409) {
            throw "HTTP idempotency conflict contract failed"
        }

        $clarificationBody = @{
            text = "1 ly cơm trắng"
            locale = "vi-VN"
            mode = "balanced"
        } | ConvertTo-Json
        $clarification = Invoke-RestMethod `
            -Method Post `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses" `
            -Headers $authHeaders `
            -ContentType "application/json; charset=utf-8" `
            -Body ([Text.Encoding]::UTF8.GetBytes($clarificationBody)) `
            -TimeoutSec 5
        $answerBody = @{
            expected_revision_id = $clarification.revision_id
            question_id = $clarification.question.id
            option_id = "unit:bát"
            mass_g = $null
        } | ConvertTo-Json
        $answered = Invoke-RestMethod `
            -Method Post `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses/$($clarification.analysis_id)/clarifications" `
            -Headers $authHeaders `
            -ContentType "application/json; charset=utf-8" `
            -Body ([Text.Encoding]::UTF8.GetBytes($answerBody)) `
            -TimeoutSec 5
        if (
            $clarification.status -ne "needs_clarification" -or
            $answered.status -ne "completed" -or
            $answered.revision_number -ne 2 -or
            $answered.items[0].estimated_mass_g -ne 150
        ) {
            throw "HTTP clarification revision contract failed"
        }

        $correctionBody = @{
            base_revision_id = $created.revision_id
            item_corrections = @(
                @{
                    item_index = 0
                    quantity = 1
                    unit = "quả"
                }
            )
        } | ConvertTo-Json -Depth 4
        $corrected = Invoke-RestMethod `
            -Method Post `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses/$($created.analysis_id)/corrections" `
            -Headers $correctionHeaders `
            -ContentType "application/json; charset=utf-8" `
            -Body ([Text.Encoding]::UTF8.GetBytes($correctionBody)) `
            -TimeoutSec 5
        $correctionReplay = Invoke-RestMethod `
            -Method Post `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses/$($created.analysis_id)/corrections" `
            -Headers $correctionHeaders `
            -ContentType "application/json; charset=utf-8" `
            -Body ([Text.Encoding]::UTF8.GetBytes($correctionBody)) `
            -TimeoutSec 5
        $originalRevision = Invoke-RestMethod `
            -Uri "http://127.0.0.1:8080/v1/nutrition/analyses/$($created.analysis_id)/revisions/1" `
            -Headers $authHeaders `
            -TimeoutSec 5
        if (
            $corrected.revision_number -ne 2 -or
            $corrected.items[0].estimated_mass_g -ne 50 -or
            $correctionReplay.revision_id -ne $corrected.revision_id -or
            $originalRevision.revision_id -ne $created.revision_id
        ) {
            throw "HTTP correction/history/idempotency contract failed"
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
