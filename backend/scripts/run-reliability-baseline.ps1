[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateRange(2, 128)]
    [int]$Concurrency,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [string]$DatabaseUrl = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition",
    [string]$ApiBaseUrl = "http://127.0.0.1:8080",
    [switch]$UseExistingServices
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$outputFullPath = [IO.Path]::GetFullPath($OutputPath)
$repositoryPrefix = $repositoryRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
if ($outputFullPath.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "OutputPath must be outside the repository"
}

$apiUri = [Uri]$ApiBaseUrl
if ($apiUri.Scheme -ne "http" -or $apiUri.Host -notin @("127.0.0.1", "localhost", "::1")) {
    throw "ApiBaseUrl must target a loopback HTTP service"
}
$databaseUri = [Uri]$DatabaseUrl
if ($databaseUri.Host -notin @("127.0.0.1", "localhost", "::1")) {
    throw "DatabaseUrl must target a loopback PostgreSQL service"
}

$apiBase = $ApiBaseUrl.TrimEnd('/')
$runId = [guid]::NewGuid().ToString("N")
$apiProcess = $null
$postgresStarted = $false
$report = [ordered]@{
    schema_version = "reliability-baseline-0.1.0"
    result = "not_started"
    run_id = $runId
    local_only = $true
    production_capacity_target = $null
    requested_concurrency = $Concurrency
    observations = @()
    verification = [ordered]@{}
}

function Invoke-CheckedNative {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Label
    )
    Write-Output "Running $Label..."
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE"
    }
}

function Wait-ApiReady {
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        try {
            $health = Invoke-RestMethod -Uri "$apiBase/health/ready" -TimeoutSec 1
            if ($health.status -eq "ready") {
                return
            }
        }
        catch {
            Start-Sleep -Milliseconds 250
        }
    }
    throw "local API did not become ready"
}

function Invoke-ConcurrentPost {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Body,
        [Parameter(Mandatory = $true)][scriptblock]$HeaderFactory
    )

    $client = [Net.Http.HttpClient]::new()
    $requests = [Collections.Generic.List[Net.Http.HttpRequestMessage]]::new()
    $tasks = [Collections.Generic.List[Threading.Tasks.Task[Net.Http.HttpResponseMessage]]]::new()
    try {
        for ($index = 0; $index -lt $Concurrency; $index++) {
            $request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Post, $Uri)
            $request.Content = [Net.Http.StringContent]::new(
                $Body,
                [Text.Encoding]::UTF8,
                "application/json"
            )
            $headers = & $HeaderFactory $index
            foreach ($key in $headers.Keys) {
                [void]$request.Headers.TryAddWithoutValidation([string]$key, [string]$headers[$key])
            }
            $requests.Add($request)
            $tasks.Add($client.SendAsync($request))
        }

        $results = [Collections.Generic.List[object]]::new()
        for ($index = 0; $index -lt $tasks.Count; $index++) {
            $response = $tasks[$index].GetAwaiter().GetResult()
            try {
                $responseBody = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
                $results.Add([pscustomobject]@{
                    request_index = $index
                    status_code = [int]$response.StatusCode
                    body = $responseBody
                })
            }
            finally {
                $response.Dispose()
            }
        }
        return @($results)
    }
    finally {
        foreach ($request in $requests) {
            $request.Dispose()
        }
        $client.Dispose()
    }
}

function Get-StatusCounts {
    param([object[]]$Results)
    $counts = [ordered]@{}
    foreach ($group in @($Results | Group-Object -Property status_code)) {
        $counts[[string]$group.Name] = [int]$group.Count
    }
    return $counts
}

function Get-JsonBody {
    param([object]$Result)
    try {
        return $Result.body | ConvertFrom-Json
    }
    catch {
        return $null
    }
}

function Get-ErrorCodes {
    param([object[]]$Results)
    $codes = [Collections.Generic.List[string]]::new()
    foreach ($result in $Results) {
        $value = Get-JsonBody $result
        if ($null -ne $value -and $null -ne $value.error) {
            $codes.Add([string]$value.error.code)
        }
    }
    return @($codes)
}

function Get-FirstSuccessfulBody {
    param([object[]]$Results)
    foreach ($result in $Results) {
        if ($result.status_code -ge 200 -and $result.status_code -lt 300) {
            return Get-JsonBody $result
        }
    }
    return $null
}

try {
    $env:APP_ENV = "ci"
    $env:AUTH_MODE = "development"
    $env:PARSER_MODE = "fixture"
    $env:DATABASE_URL = $DatabaseUrl
    $env:TEST_DATABASE_URL = $DatabaseUrl

    if (-not $UseExistingServices) {
        Invoke-CheckedNative "docker" @("compose", "-f", "deploy/compose.yaml", "up", "-d", "--wait", "postgres") "local PostgreSQL startup"
        $postgresStarted = $true

        $env:RUN_MIGRATIONS = "true"
        $env:RUN_FOUNDATION_SEED = "true"
        Invoke-CheckedNative "cargo" @("run", "-p", "worker") "local migration and fixture seed"
        $env:RUN_MIGRATIONS = "false"
        $env:RUN_FOUNDATION_SEED = "false"

        Invoke-CheckedNative "cargo" @("test", "-p", "persistence-postgres", "--test", "postgres_integration", "--", "--ignored", "--test-threads=1") "PostgreSQL reliability integration tests"
    }
    else {
        Write-Output "Using caller-managed local PostgreSQL and API services."
    }

    Invoke-CheckedNative "cargo" @("test", "-p", "adapters", "retries_timeout_once_then_fails_closed") "parser timeout/fail-closed test"
    Invoke-CheckedNative "cargo" @("test", "-p", "adapters", "opens_circuit_after_bounded_transient_retry") "parser bounded retry/circuit test"

    if (-not $UseExistingServices) {
        Invoke-CheckedNative "cargo" @("build", "-p", "api-http") "local API build"
        $apiBinaryName = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) { "api-http.exe" } else { "api-http" }
        $apiPath = (Resolve-Path (Join-Path $repositoryRoot (Join-Path "target/debug" $apiBinaryName))).Path
        $apiProcess = Start-Process -FilePath $apiPath -PassThru -WindowStyle Hidden
    }

    Wait-ApiReady
    $authorization = "Bearer dev:0198f100-0000-7000-8000-000000000098"
    $commonHeaders = @{ Authorization = $authorization }

    $createBody = @{
        text = "2 quả trứng gà luộc, 1 bát cơm trắng"
        locale = "vi-VN"
        mode = "balanced"
    } | ConvertTo-Json -Compress
    $createKey = "reliability-create-$runId"
    $createResults = Invoke-ConcurrentPost `
        -Uri "$apiBase/v1/nutrition/analyses" `
        -Body $createBody `
        -HeaderFactory {
            param($index)
            return @{ Authorization = $authorization; "Idempotency-Key" = $createKey }
        }
    $createReplay = Invoke-RestMethod `
        -Method Post `
        -Uri "$apiBase/v1/nutrition/analyses" `
        -Headers @{ Authorization = $authorization; "Idempotency-Key" = $createKey } `
        -ContentType "application/json; charset=utf-8" `
        -Body ([Text.Encoding]::UTF8.GetBytes($createBody)) `
        -TimeoutSec 5
    $successfulCreateBodies = @($createResults | ForEach-Object {
        if ($_.status_code -ge 200 -and $_.status_code -lt 300) { Get-JsonBody $_ }
    })
    $createIds = @($successfulCreateBodies | ForEach-Object { [string]$_.analysis_id } | Where-Object { $_ })
    $concurrentCreateSuccesses = @($createResults | Where-Object { $_.status_code -ge 200 -and $_.status_code -lt 300 })
    $concurrentCreateFailures = @($createResults | Where-Object { $_.status_code -lt 200 -or $_.status_code -ge 300 })
    $createIdsConverged = $createIds.Count -gt 0 -and (@($createIds | Select-Object -Unique).Count -eq 1)
    $createObservedContract = if ($createIdsConverged -and $concurrentCreateFailures.Count -eq 0 -and [string]$createReplay.analysis_id -eq [string]$createIds[0]) {
        "converged"
    }
    elseif ($createIdsConverged -and $concurrentCreateSuccesses.Count -eq 1 -and [string]$createReplay.analysis_id -eq [string]$createIds[0]) {
        "winner_with_concurrent_failures"
    }
    else {
        "not_converged"
    }
    $report.observations += [ordered]@{
        scenario = "concurrent_create_idempotency"
        request_count = $Concurrency
        status_counts = Get-StatusCounts $createResults
        error_codes = Get-ErrorCodes $createResults
        concurrent_success_count = $concurrentCreateSuccesses.Count
        concurrent_failure_count = $concurrentCreateFailures.Count
        successful_analysis_ids = $createIds
        replay_analysis_id = [string]$createReplay.analysis_id
        replay_status = "200"
        observed_contract = $createObservedContract
        interpretation = "Observed outcome only; this is not a production capacity target."
    }

    $correctionSeed = Invoke-RestMethod `
        -Method Post `
        -Uri "$apiBase/v1/nutrition/analyses" `
        -Headers @{ Authorization = $authorization; "Idempotency-Key" = "reliability-correction-seed-$runId" } `
        -ContentType "application/json; charset=utf-8" `
        -Body ([Text.Encoding]::UTF8.GetBytes($createBody)) `
        -TimeoutSec 5
    $correctionBody = @{
        base_revision_id = $correctionSeed.revision_id
        item_corrections = @(@{ item_index = 0; quantity = 1; unit = "quả" })
    } | ConvertTo-Json -Depth 5 -Compress
    $correctionResults = Invoke-ConcurrentPost `
        -Uri "$apiBase/v1/nutrition/analyses/$($correctionSeed.analysis_id)/corrections" `
        -Body $correctionBody `
        -HeaderFactory {
            param($index)
            return @{ Authorization = $authorization; "Idempotency-Key" = "reliability-correction-$runId-$index" }
        }
    $correctionSuccesses = @($correctionResults | Where-Object { $_.status_code -eq 200 })
    $correctionConflicts = @($correctionResults | Where-Object { $_.status_code -eq 409 })
    $report.observations += [ordered]@{
        scenario = "concurrent_correction_revision_conflict"
        request_count = $Concurrency
        status_counts = Get-StatusCounts $correctionResults
        error_codes = Get-ErrorCodes $correctionResults
        observed_contract = if ($correctionSuccesses.Count -eq 1 -and $correctionConflicts.Count -eq ($Concurrency - 1)) { "one_revision_wins" } else { "different_or_incomplete" }
        interpretation = "A single current revision may win; stale callers are measured as conflicts."
    }

    $clarificationSeedBody = @{
        text = "1 ly cơm trắng"
        locale = "vi-VN"
        mode = "balanced"
    } | ConvertTo-Json -Compress
    $clarificationSeed = Invoke-RestMethod `
        -Method Post `
        -Uri "$apiBase/v1/nutrition/analyses" `
        -Headers $commonHeaders `
        -ContentType "application/json; charset=utf-8" `
        -Body ([Text.Encoding]::UTF8.GetBytes($clarificationSeedBody)) `
        -TimeoutSec 5
    $clarificationBody = @{
        expected_revision_id = $clarificationSeed.revision_id
        question_id = $clarificationSeed.question.id
        option_id = "unit:bát"
        mass_g = $null
    } | ConvertTo-Json -Depth 5 -Compress
    $clarificationResults = Invoke-ConcurrentPost `
        -Uri "$apiBase/v1/nutrition/analyses/$($clarificationSeed.analysis_id)/clarifications" `
        -Body $clarificationBody `
        -HeaderFactory {
            param($index)
            return @{ Authorization = $authorization; "X-Request-Id" = "reliability-clarification-$runId-$index" }
        }
    $clarificationSuccesses = @($clarificationResults | Where-Object { $_.status_code -eq 200 })
    $clarificationConflicts = @($clarificationResults | Where-Object { $_.status_code -eq 409 })
    $report.observations += [ordered]@{
        scenario = "concurrent_clarification_revision_conflict"
        request_count = $Concurrency
        status_counts = Get-StatusCounts $clarificationResults
        error_codes = Get-ErrorCodes $clarificationResults
        observed_contract = if ($clarificationSuccesses.Count -eq 1 -and $clarificationConflicts.Count -eq ($Concurrency - 1)) { "one_revision_wins" } else { "different_or_incomplete" }
        interpretation = "A single open clarification may be answered; stale answers are measured as conflicts."
    }

    $report.verification = [ordered]@{
        local_postgres_integration = if ($UseExistingServices) { "caller_managed" } else { "passed" }
        parser_timeout_and_failure = "passed"
        api_service = "ready"
        hosted_provider_called = $false
        production_credentials_used = $false
    }
    $report.result = "pass"
}
catch {
    $report.result = "failed"
    $report.failure = $_.Exception.Message
    throw
}
finally {
    if ($apiProcess -and -not $apiProcess.HasExited) {
        Stop-Process -Id $apiProcess.Id
    }
    if ($postgresStarted) {
        & docker compose -f deploy/compose.yaml stop postgres
    }
    $parentDirectory = Split-Path -Parent $outputFullPath
    if (-not (Test-Path -LiteralPath $parentDirectory)) {
        New-Item -ItemType Directory -Path $parentDirectory -Force | Out-Null
    }
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $outputFullPath -Encoding utf8
}
