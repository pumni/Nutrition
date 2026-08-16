param(
    [Parameter(Mandatory = $true)]
    [string]$ExpectedPath,
    [Parameter(Mandatory = $true)]
    [string]$PredictionsPath,
    [ValidateSet("development", "public_test", "sealed_test", "challenge")]
    [string]$SplitName = "public_test",
    [string]$ManifestPath = (Join-Path $PSScriptRoot "..\fixtures\vietnamese-meal-bench\manifest.json"),
    [string]$OutputPath
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

function Normalize-Text {
    param([AllowNull()][string]$Value)
    if ($null -eq $Value) { return "" }
    return ([regex]::Replace($Value.Trim().ToLowerInvariant(), "\s+", " "))
}

function Get-RootArray {
    param($Root, [string]$PropertyName)
    if ($Root -is [System.Array]) { return @($Root) }
    if ($null -eq $Root.$PropertyName) { throw "JSON root must be an array or contain .$PropertyName" }
    return @($Root.$PropertyName)
}

function Convert-Comparable {
    param($Value)
    return ($Value | ConvertTo-Json -Depth 20 -Compress)
}

function Test-PredictionShape {
    param($Prediction)
    if ([string]::IsNullOrWhiteSpace([string]$Prediction.sample_id) -or
        [string]$Prediction.parse_decision -notin @("parsed", "parse_rejected") -or
        [string]$Prediction.analysis_decision -notin @("resolve", "needs_clarification", "insufficient") -or
        $null -eq $Prediction.items -or $null -eq $Prediction.safety_flags) {
        return $false
    }
    foreach ($item in @($Prediction.items)) {
        if ([string]::IsNullOrWhiteSpace([string]$item.food_phrase) -or
            $null -eq $item.preparation -or $null -eq $item.modifiers -or
            $null -eq $item.negated) {
            return $false
        }
    }
    return $true
}

function Get-PhraseCounts {
    param($Items)
    $counts = @{}
    foreach ($item in @($Items)) {
        $key = Normalize-Text ([string]$item.food_phrase)
        if (-not $counts.ContainsKey($key)) { $counts[$key] = 0 }
        $counts[$key]++
    }
    return $counts
}

function Get-IntersectionCount {
    param($Left, $Right)
    $total = 0
    foreach ($key in $Left.Keys) {
        if ($Right.ContainsKey($key)) { $total += [math]::Min($Left[$key], $Right[$key]) }
    }
    return $total
}

$manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
$expectedRoot = Get-Content -Raw -LiteralPath $ExpectedPath | ConvertFrom-Json
$predictionRoot = Get-Content -Raw -LiteralPath $PredictionsPath | ConvertFrom-Json
$expectedCases = Get-RootArray $expectedRoot "cases"
$predictions = Get-RootArray $predictionRoot "predictions"
if ($expectedCases.Count -eq 0) { throw "expected cases must not be empty" }

$expectedMap = @{}
foreach ($case in $expectedCases) {
    $id = [string]$case.sample_id
    if ($expectedMap.ContainsKey($id)) { throw "expected cases contain duplicate sample_id $id" }
    $expectedMap[$id] = $case
}
$predictionMap = @{}
foreach ($prediction in $predictions) {
    $id = [string]$prediction.sample_id
    if ($predictionMap.ContainsKey($id)) { throw "predictions contain duplicate sample_id $id" }
    $predictionMap[$id] = $prediction
}

$schemaValid = 0
$parseCorrect = 0
$analysisCorrect = 0
$safetyCorrect = 0
$truePositives = 0
$predictedMentions = 0
$expectedMentions = 0
$falseResolveCount = 0
$sliceData = @{}
$missing = 0

foreach ($id in $expectedMap.Keys) {
    $expected = $expectedMap[$id]
    if (-not $predictionMap.ContainsKey($id)) { $missing++; continue }
    $prediction = $predictionMap[$id]
    if (-not (Test-PredictionShape $prediction)) { continue }
    $schemaValid++

    $expectedParse = [string]$expected.expected_parse_decision
    $expectedAnalysis = [string]$expected.expected_analysis_decision
    if ([string]$prediction.parse_decision -eq $expectedParse) { $parseCorrect++ }
    $expectedDimension = [string]$expected.expected_analysis_clarification_dimension
    $predictionDimension = [string]$prediction.analysis_clarification_dimension
    if ([string]$prediction.analysis_decision -eq $expectedAnalysis -and
        ($expectedAnalysis -ne "needs_clarification" -or $predictionDimension -eq $expectedDimension)) {
        $analysisCorrect++
    }
    if ((Convert-Comparable @($prediction.safety_flags | Sort-Object)) -eq
        (Convert-Comparable @($expected.expected_parse.safety_flags | Sort-Object))) {
        $safetyCorrect++
    }

    $expectedPhrases = Get-PhraseCounts @($expected.expected_parse.items)
    $predictionPhrases = Get-PhraseCounts @($prediction.items)
    $truePositives += Get-IntersectionCount $expectedPhrases $predictionPhrases
    $predictedMentions += @($prediction.items).Count
    $expectedMentions += @($expected.expected_parse.items).Count
    if ([string]$prediction.analysis_decision -eq "resolve" -and $expectedAnalysis -ne "resolve") {
        $falseResolveCount++
    }

    foreach ($tag in @($expected.tags)) {
        $tagKey = [string]$tag
        if (-not $sliceData.ContainsKey($tagKey)) {
            $sliceData[$tagKey] = [ordered]@{total = 0; schema_valid = 0; parse_correct = 0; analysis_correct = 0}
        }
        $sliceData[$tagKey].total++
        $sliceData[$tagKey].schema_valid++
        if ([string]$prediction.parse_decision -eq $expectedParse) { $sliceData[$tagKey].parse_correct++ }
        if ([string]$prediction.analysis_decision -eq $expectedAnalysis -and
            ($expectedAnalysis -ne "needs_clarification" -or $predictionDimension -eq $expectedDimension)) {
            $sliceData[$tagKey].analysis_correct++
        }
    }
}
$unexpected = @($predictionMap.Keys | Where-Object { -not $expectedMap.ContainsKey($_) }).Count

$total = $expectedCases.Count
$mentionPrecision = if ($predictedMentions -eq 0) { 0 } else { $truePositives / $predictedMentions }
$mentionRecall = if ($expectedMentions -eq 0) { 0 } else { $truePositives / $expectedMentions }
$mentionF1 = if (($mentionPrecision + $mentionRecall) -eq 0) { 0 } else { 2 * $mentionPrecision * $mentionRecall / ($mentionPrecision + $mentionRecall) }
$metrics = [ordered]@{
    schema_valid_rate = [math]::Round($schemaValid / $total, 6)
    parse_decision_accuracy = [math]::Round($parseCorrect / $total, 6)
    analysis_decision_accuracy = [math]::Round($analysisCorrect / $total, 6)
    safety_flags_exact_rate = [math]::Round($safetyCorrect / $total, 6)
    mention_precision = [math]::Round($mentionPrecision, 6)
    mention_recall = [math]::Round($mentionRecall, 6)
    mention_f1 = [math]::Round($mentionF1, 6)
    over_resolution_rate = [math]::Round($falseResolveCount / $total, 6)
    known_food_top3_recall = $null
    known_food_top1_accuracy = $null
    unknown_detection_precision = $null
    calculation_fixture_pass_rate = $null
    replay_pass_rate = $null
}

$thresholds = [ordered]@{
    threshold_status = [string]$manifest.metrics.threshold_status
    profile = [ordered]@{
        schema_valid_rate = $manifest.metrics.schema_valid_rate.walking_skeleton
        mention_f1 = $manifest.metrics.mention_f1.walking_skeleton
        over_resolution_rate = $manifest.metrics.over_resolution_rate.walking_skeleton
        calculation_fixture_pass_rate = $manifest.metrics.calculation_fixture_pass_rate.walking_skeleton
        replay_pass_rate = $manifest.metrics.replay_pass_rate.walking_skeleton
    }
    not_scored = @("known_food_top3_recall", "known_food_top1_accuracy", "unknown_detection_precision", "calculation_fixture_pass_rate", "replay_pass_rate")
}

$report = [ordered]@{
    benchmark = [string]$manifest.benchmark
    benchmark_version = [string]$manifest.version
    split = $SplitName
    expected_file_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $ExpectedPath).Hash.ToLowerInvariant()
    predictions_file_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $PredictionsPath).Hash.ToLowerInvariant()
    counts = [ordered]@{expected = $total; predictions = $predictions.Count; schema_valid = $schemaValid; missing_predictions = $missing; unexpected_predictions = $unexpected}
    metrics = $metrics
    slices = [ordered]@{}
    thresholds = $thresholds
    release_gate = [ordered]@{
        human_adjudication = "blocked_until_independent_review_and_domain_adjudication"
        analysis_gold = "blocked_until_catalog_portion_and_behavior_versions"
        sealed_test = "not_verified"
        challenge = "not_verified"
        production_eligible = $false
    }
    note = "Aggregate report only. It does not approve annotations, load sealed answers, or enable production."
}
foreach ($tag in ($sliceData.Keys | Sort-Object)) {
    $data = $sliceData[$tag]
    $report.slices[$tag] = [ordered]@{
        case_count = $data.total
        schema_valid_rate = [math]::Round($data.schema_valid / $data.total, 6)
        parse_decision_accuracy = [math]::Round($data.parse_correct / $data.total, 6)
        analysis_decision_accuracy = [math]::Round($data.analysis_correct / $data.total, 6)
    }
}

$json = $report | ConvertTo-Json -Depth 30
if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
    $json | Set-Content -LiteralPath $OutputPath -Encoding utf8
}
Write-Output $json
