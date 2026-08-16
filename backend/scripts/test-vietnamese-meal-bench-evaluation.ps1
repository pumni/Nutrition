$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$publicCasesPath = Join-Path $repositoryRoot "fixtures\vietnamese-meal-bench\public-test-cases.json"
$prepareScript = Join-Path $PSScriptRoot "prepare-vietnamese-meal-bench-adjudication.ps1"
$compareScript = Join-Path $PSScriptRoot "compare-vietnamese-meal-bench-annotations.ps1"
$scoreScript = Join-Path $PSScriptRoot "score-vietnamese-meal-bench.ps1"
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("vmb-evaluation-test-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null

function Write-JsonFile {
    param($Value, [string]$Path)
    $Value | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $Path -Encoding utf8
}

function Invoke-Score {
    param($Predictions, [string]$Name)
    $predictionPath = Join-Path $tempRoot "$Name-predictions.json"
    $reportPath = Join-Path $tempRoot "$Name-report.json"
    Write-JsonFile ([ordered]@{predictions = $Predictions}) $predictionPath
    & $scoreScript -ExpectedPath $publicCasesPath -PredictionsPath $predictionPath -SplitName public_test -OutputPath $reportPath | Out-Null
    return Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json
}

function Copy-Predictions {
    param($Cases)
    $predictions = @()
    foreach ($case in $Cases) {
        $itemsCopy = ConvertFrom-Json (ConvertTo-Json -InputObject @($case.expected_parse.items) -Depth 30)
        $predictions += [pscustomobject][ordered]@{
            sample_id = [string]$case.sample_id
            parse_decision = [string]$case.expected_parse_decision
            items = @($itemsCopy)
            analysis_decision = [string]$case.expected_analysis_decision
            analysis_clarification_dimension = if ($case.expected_analysis_decision -eq "needs_clarification") {
                [string]$case.expected_analysis_clarification_dimension
            } else {
                $null
            }
            safety_flags = @($case.expected_parse.safety_flags)
        }
    }
    return $predictions
}

function Assert-ComparatorRejects {
    param([string]$Name, [scriptblock]$Mutation)
    $variantA = Get-Content -Raw -LiteralPath $packetAPath | ConvertFrom-Json
    $variantB = Get-Content -Raw -LiteralPath $packetBPath | ConvertFrom-Json
    & $Mutation $variantA $variantB
    $variantAPath = Join-Path $tempRoot "$Name-a.json"
    $variantBPath = Join-Path $tempRoot "$Name-b.json"
    $variantOutputPath = Join-Path $tempRoot "$Name-comparison.json"
    Write-JsonFile $variantA $variantAPath
    Write-JsonFile $variantB $variantBPath
    $rejected = $false
    try {
        & $compareScript -AnnotatorAPath $variantAPath -AnnotatorBPath $variantBPath -OutputPath $variantOutputPath | Out-Null
    } catch {
        $rejected = $true
    }
    if (-not $rejected) { throw "$Name comparator mutation was accepted" }
}

try {
    $packetsPath = Join-Path $tempRoot "packets"
    & $prepareScript -InputPath $publicCasesPath -OutputDirectory $packetsPath | Out-Null
    $packetAPath = Join-Path $packetsPath "independent-a.json"
    $packetBPath = Join-Path $packetsPath "independent-b.json"
    $expectedCases = @(Get-Content -Raw -LiteralPath $publicCasesPath | ConvertFrom-Json)
    $packetA = Get-Content -Raw -LiteralPath $packetAPath | ConvertFrom-Json
    $packetB = Get-Content -Raw -LiteralPath $packetBPath | ConvertFrom-Json
    foreach ($packet in @($packetA, $packetB)) {
        foreach ($case in @($packet.cases)) {
            $gold = $expectedCases | Where-Object { $_.sample_id -eq $case.sample_id }
            $case.annotation.parse_decision = [string]$gold.expected_parse_decision
            $case.annotation.items = @($gold.expected_parse.items)
            $case.annotation.safety_flags = @($gold.expected_parse.safety_flags)
        }
    }
    Write-JsonFile $packetA $packetAPath
    Write-JsonFile $packetB $packetBPath

    $comparisonPath = Join-Path $tempRoot "comparison.json"
    & $compareScript -AnnotatorAPath $packetAPath -AnnotatorBPath $packetBPath -OutputPath $comparisonPath | Out-Null
    $comparison = Get-Content -Raw -LiteralPath $comparisonPath | ConvertFrom-Json
    if ($comparison.status -ne "awaiting_domain_adjudicator" -or
        $comparison.disagreement_count -ne 0 -or
        [string]::IsNullOrWhiteSpace([string]$comparison.annotator_a_packet_sha256) -or
        [string]::IsNullOrWhiteSpace([string]$comparison.annotator_b_packet_sha256)) {
        throw "completed identical annotations did not produce hashed pending evidence"
    }

    $blankRejected = $false
    try {
        $blankAPath = Join-Path $tempRoot "blank-a.json"
        $blankBPath = Join-Path $tempRoot "blank-b.json"
        & $prepareScript -InputPath $publicCasesPath -OutputDirectory (Join-Path $tempRoot "blank-packets") | Out-Null
        Copy-Item (Join-Path $tempRoot "blank-packets\independent-a.json") $blankAPath
        Copy-Item (Join-Path $tempRoot "blank-packets\independent-b.json") $blankBPath
        & $compareScript -AnnotatorAPath $blankAPath -AnnotatorBPath $blankBPath -OutputPath (Join-Path $tempRoot "blank-comparison.json") | Out-Null
    } catch {
        $blankRejected = $true
    }
    if (-not $blankRejected) { throw "blank annotation packets were accepted" }

    Assert-ComparatorRejects "text-mismatch" {
        param($variantA, $variantB)
        $variantB.cases[0].text = "$($variantB.cases[0].text) khác"
    }
    Assert-ComparatorRejects "locale-mismatch" {
        param($variantA, $variantB)
        $variantB.cases[0].locale = "en-US"
    }
    $safetyCase = $expectedCases | Where-Object { @($_.expected_parse.safety_flags).Count -gt 0 } | Select-Object -First 1
    if ($null -eq $safetyCase) { throw "evaluation fixtures must contain a safety-flag case" }
    Assert-ComparatorRejects "duplicate-safety-flag" {
        param($variantA, $variantB)
        $caseA = $variantA.cases | Where-Object { $_.sample_id -eq $safetyCase.sample_id }
        $caseA.annotation.safety_flags = @($caseA.annotation.safety_flags) + @([string]$caseA.annotation.safety_flags[0])
    }
    $normalizedSafetyAPath = Join-Path $tempRoot "normalized-safety-a.json"
    $normalizedSafetyBPath = Join-Path $tempRoot "normalized-safety-b.json"
    $normalizedSafetyReportPath = Join-Path $tempRoot "normalized-safety-comparison.json"
    $normalizedSafetyA = Get-Content -Raw -LiteralPath $packetAPath | ConvertFrom-Json
    $normalizedSafetyB = Get-Content -Raw -LiteralPath $packetBPath | ConvertFrom-Json
    $normalizedCaseA = $normalizedSafetyA.cases | Where-Object { $_.sample_id -eq $safetyCase.sample_id }
    $normalizedCaseB = $normalizedSafetyB.cases | Where-Object { $_.sample_id -eq $safetyCase.sample_id }
    $normalizedCaseA.annotation.safety_flags = @("Safety Flag")
    $normalizedCaseB.annotation.safety_flags = @("  safety   flag ")
    Write-JsonFile $normalizedSafetyA $normalizedSafetyAPath
    Write-JsonFile $normalizedSafetyB $normalizedSafetyBPath
    & $compareScript -AnnotatorAPath $normalizedSafetyAPath -AnnotatorBPath $normalizedSafetyBPath -OutputPath $normalizedSafetyReportPath | Out-Null
    $normalizedSafetyReport = Get-Content -Raw -LiteralPath $normalizedSafetyReportPath | ConvertFrom-Json
    if ($normalizedSafetyReport.disagreement_count -ne 0) {
        throw "normalized safety flags were not compared as a set"
    }

    $validPredictions = Copy-Predictions $expectedCases
    $missingAndInvalid = @($validPredictions | Where-Object {
        $_.sample_id -ne $expectedCases[0].sample_id -and $_.sample_id -ne $expectedCases[1].sample_id
    })
    $invalidExtra = $validPredictions | Where-Object { $_.sample_id -eq $expectedCases[1].sample_id }
    Add-Member -InputObject $invalidExtra -NotePropertyName unexpected -NotePropertyValue "reject" -Force
    $missingAndInvalid += $invalidExtra
    $denominatorReport = Invoke-Score $missingAndInvalid "missing-invalid"
    if ($denominatorReport.counts.missing_predictions -ne 1 -or
        $denominatorReport.counts.schema_invalid -ne 1 -or
        $denominatorReport.metrics.schema_valid_rate -ge 1 -or
        $denominatorReport.metrics.mention_recall -ge 1 -or
        $denominatorReport.slices."common-meal".schema_valid_rate -ge 1 -or
        $denominatorReport.slices."common-meal".mention_recall -ge 1) {
        throw "missing or invalid predictions were not retained in aggregate and slice denominators"
    }

    $parsedCase = $expectedCases | Where-Object { $_.expected_parse_decision -eq "parsed" } | Select-Object -First 1
    $rejectedCase = $expectedCases | Where-Object { $_.expected_parse_decision -eq "parse_rejected" } | Select-Object -First 1
    $clarificationCase = $expectedCases | Where-Object { $_.expected_analysis_decision -eq "needs_clarification" } | Select-Object -First 1
    $resolveCase = $expectedCases | Where-Object { $_.expected_analysis_decision -eq "resolve" } | Select-Object -First 1
    $predictionSafetyCase = $expectedCases | Where-Object { @($_.expected_parse.safety_flags).Count -gt 0 } | Select-Object -First 1
    foreach ($variantName in @(
        "extra-property", "wrong-type", "missing-field", "malformed-item",
        "parsed-empty-items", "rejected-nonzero-items", "rejected-non-insufficient",
        "clarification-empty-dimension", "resolve-nonnull-dimension", "duplicate-safety-flag"
    )) {
        $variant = Copy-Predictions $expectedCases
        $target = $variant | Where-Object { $_.sample_id -eq $expectedCases[0].sample_id }
        switch ($variantName) {
            "extra-property" { Add-Member -InputObject $target -NotePropertyName unexpected -NotePropertyValue "reject" -Force }
            "wrong-type" { $target.parse_decision = 7 }
            "missing-field" { $target.PSObject.Properties.Remove("items") }
            "malformed-item" {
                $target.items[0].food_phrase = 7
            }
            "parsed-empty-items" {
                $target = $variant | Where-Object { $_.sample_id -eq $parsedCase.sample_id }
                $target.items = @()
            }
            "rejected-nonzero-items" {
                $target = $variant | Where-Object { $_.sample_id -eq $rejectedCase.sample_id }
                $target.items = @($variant | Where-Object { $_.sample_id -eq $parsedCase.sample_id } | Select-Object -ExpandProperty items | Select-Object -First 1)
            }
            "rejected-non-insufficient" {
                $target = $variant | Where-Object { $_.sample_id -eq $rejectedCase.sample_id }
                $target.analysis_decision = "resolve"
            }
            "clarification-empty-dimension" {
                $target = $variant | Where-Object { $_.sample_id -eq $clarificationCase.sample_id }
                $target.analysis_clarification_dimension = " "
            }
            "resolve-nonnull-dimension" {
                $target = $variant | Where-Object { $_.sample_id -eq $resolveCase.sample_id }
                $target.analysis_clarification_dimension = "food_identity"
            }
            "duplicate-safety-flag" {
                $target = $variant | Where-Object { $_.sample_id -eq $predictionSafetyCase.sample_id }
                $target.safety_flags = @($target.safety_flags) + @([string]$target.safety_flags[0])
            }
        }
        $variantReport = Invoke-Score $variant $variantName
        if ($variantReport.counts.schema_invalid -ne 1) {
            throw "$variantName did not fail the authoritative prediction schema contract"
        }
    }

    Write-Output "VIETNAMESE_MEAL_BENCH_EVALUATION_REGRESSIONS_PASS"
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force
}
