[CmdletBinding()]
param(
    [ValidateRange(20, 2000)]
    [int]$RequestCount = 100,
    [ValidateRange(1, 32)]
    [int]$Concurrency = 4,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [string]$DatabaseUrl = "postgres://nutrition:nutrition@127.0.0.1:5432/nutrition",
    [switch]$UseExistingServices
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$backendRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$repositoryRoot = (Resolve-Path (Join-Path $backendRoot "..")).Path
$outputFullPath = [IO.Path]::GetFullPath($OutputPath)
$repositoryPrefix = $repositoryRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
if ($outputFullPath.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "OutputPath must be outside the repository"
}

$databaseUri = [Uri]$DatabaseUrl
if ($databaseUri.Scheme -notin @("postgres", "postgresql") -or
    $databaseUri.Host -notin @("127.0.0.1", "localhost", "::1")) {
    throw "DatabaseUrl must target loopback PostgreSQL"
}

$outputDirectory = Split-Path -Parent $outputFullPath
if (-not (Test-Path -LiteralPath $outputDirectory)) {
    New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
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

$postgresStarted = $false
Push-Location $backendRoot
try {
    $env:APP_ENV = "ci"
    $env:AUTH_MODE = "development"
    $env:PARSER_MODE = "fixture"
    $env:DATABASE_URL = $DatabaseUrl
    $env:TEST_DATABASE_URL = $DatabaseUrl

    if (-not $UseExistingServices) {
        $runningServices = & docker compose -f deploy/compose.yaml ps --status running --services
        if ($LASTEXITCODE -ne 0) {
            throw "Could not inspect local PostgreSQL compose service"
        }
        $alreadyRunning = @($runningServices | Where-Object { $_ -eq "postgres" }).Count -gt 0
        if (-not $alreadyRunning) {
            Invoke-CheckedNative "docker" @("compose", "-f", "deploy/compose.yaml", "up", "-d", "--wait", "postgres") "local PostgreSQL startup"
            $postgresStarted = $true
        }

        $env:RUN_MIGRATIONS = "true"
        $env:RUN_FOUNDATION_SEED = "true"
        $env:WORKER_MODE = "idle"
        $env:WORKER_ID = "performance-baseline-bootstrap"
        Invoke-CheckedNative "cargo" @("run", "-p", "worker") "local migration and foundation seed"
        $env:RUN_MIGRATIONS = "false"
        $env:RUN_FOUNDATION_SEED = "false"
    }

    $env:API_DATABASE_POOL_SIZE = "8"
    $env:PERFORMANCE_BASELINE_DATASET = "foundation-fixture-seed"
    $env:PERFORMANCE_BASELINE_OUTPUT_PATH = $outputFullPath
    $env:PERFORMANCE_BASELINE_REQUEST_COUNT = [string]$RequestCount
    $env:PERFORMANCE_BASELINE_CONCURRENCY = [string]$Concurrency
    Invoke-CheckedNative "cargo" @(
        "test",
        "-p", "api-http",
        "--lib",
        "performance_baseline::write_local_baseline_report",
        "--",
        "--ignored",
        "--exact",
        "--test-threads=1"
    ) "local performance baseline measurement"

    if (-not (Test-Path -LiteralPath $outputFullPath -PathType Leaf)) {
        throw "The measurement completed without writing its report"
    }
    Write-Output "Performance baseline report written outside the repository."
}
finally {
    if ($postgresStarted) {
        & docker compose -f deploy/compose.yaml stop postgres
    }
    Pop-Location
}
