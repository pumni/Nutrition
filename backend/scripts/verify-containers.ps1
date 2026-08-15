$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$composeArguments = @(
    "-f", "deploy/compose.yaml",
    "-f", "deploy/compose.container-smoke.yaml"
)

Write-Output "Building and starting production container smoke topology..."
docker compose @composeArguments up -d --build api-smoke

try {
    $ready = $false
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        try {
            $response = Invoke-RestMethod `
                -Uri "http://127.0.0.1:18080/health/ready" `
                -TimeoutSec 1
            if ($response.status -eq "ready") {
                $ready = $true
                break
            }
        }
        catch {
            Start-Sleep -Milliseconds 500
        }
    }
    if (-not $ready) {
        docker compose @composeArguments logs api-smoke
        throw "containerized API did not become ready"
    }

    $apiUser = docker image inspect nutrition-api:ci --format '{{.Config.User}}'
    $workerUser = docker image inspect nutrition-worker:ci --format '{{.Config.User}}'
    if ($apiUser -ne "10001:10001" -or $workerUser -ne "10001:10001") {
        throw "production images must run as non-root UID/GID 10001:10001"
    }

    Write-Output "Production container verification passed."
}
finally {
    docker compose @composeArguments down -v --remove-orphans
}
