param(
    [Parameter(Mandatory = $true)]
    [string]$InputPath,
    [Parameter(Mandatory = $true)]
    [string]$OutputDirectory,
    [string]$BenchmarkVersion = "foundation-0.5.1",
    [string]$RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot ".." )).Path
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$repositoryFullPath = [System.IO.Path]::GetFullPath($RepositoryRoot).TrimEnd('\')
$outputFullPath = [System.IO.Path]::GetFullPath($OutputDirectory).TrimEnd('\')
if ($outputFullPath.Equals($repositoryFullPath, [System.StringComparison]::OrdinalIgnoreCase) -or
    $outputFullPath.StartsWith("$repositoryFullPath\", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "annotation packets must be written outside the repository"
}

$sourcePath = (Resolve-Path -LiteralPath $InputPath).Path
$sourceCases = @(Get-Content -Raw -LiteralPath $sourcePath | ConvertFrom-Json)
if ($sourceCases.Count -eq 0) {
    throw "the annotation source must contain at least one case"
}

$sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourcePath).Hash.ToLowerInvariant()
New-Item -ItemType Directory -Force -Path $outputFullPath | Out-Null

foreach ($slot in @("independent-a", "independent-b")) {
    $packet = [ordered]@{
        benchmark = "VietnameseMealBench"
        version = $BenchmarkVersion
        packet_type = "independent_parse_annotation"
        annotator_slot = $slot
        source_case_file_sha256 = $sourceHash
        guideline = "docs/evidence/vietnamese-meal-bench-adjudication.md"
        cases = @(
            foreach ($case in $sourceCases) {
                [ordered]@{
                    sample_id = [string]$case.sample_id
                    text = [string]$case.text
                    locale = [string]$case.locale
                    annotation = [ordered]@{
                        parse_decision = $null
                        items = @()
                        safety_flags = @()
                    }
                }
            }
        )
    }
    $outputPath = Join-Path $outputFullPath "$slot.json"
    $packet | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $outputPath -Encoding utf8
    Write-Output ([ordered]@{
        packet = $slot
        output = $outputPath
        case_count = $sourceCases.Count
        source_case_file_sha256 = $sourceHash
    } | ConvertTo-Json -Compress)
}
