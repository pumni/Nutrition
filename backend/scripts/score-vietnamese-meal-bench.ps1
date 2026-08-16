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
    return [regex]::Replace($Value.Trim().ToLowerInvariant(), "\s+", " ")
}

function Get-RootArray {
    param($Root, [string]$PropertyName)
    if ($Root -is [System.Array]) { return @($Root) }
    if ($null -eq $Root.$PropertyName) { throw "JSON root must be an array or contain .$PropertyName" }
    return @($Root.$PropertyName)
}

function Test-AllowedProperties {
    param($Object, [string[]]$Allowed)
    if ($null -eq $Object) { return $false }
    $actual = @($Object.PSObject.Properties | ForEach-Object { $_.Name })
    return @($actual | Where-Object { $_ -notin $Allowed }).Count -eq 0
}

function Get-NormalizedSafetyFlags {
    param($Values)
    $normalized = @()
    foreach ($value in @($Values)) {
        if ($value -isnot [string]) { return $null }
        $normalized += Normalize-Text $value
    }
    if (@($normalized | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -gt 0 -or
        @($normalized | Sort-Object -Unique).Count -ne $normalized.Count) {
        return $null
    }
    return ,(@($normalized | Sort-Object))
}

function Test-PredictionSchema {
    param($Prediction)
    $errors = @()
    if (-not (Test-AllowedProperties $Prediction @("sample_id", "parse_decision", "items", "analysis_decision", "analysis_clarification_dimension", "safety_flags"))) {
        $errors += "unexpected prediction properties"
    }
    if ($Prediction.sample_id -isnot [string] -or [string]::IsNullOrWhiteSpace($Prediction.sample_id)) {
        $errors += "sample_id must be a non-empty string"
    }
    if ($Prediction.parse_decision -isnot [string] -or
        $Prediction.parse_decision -notin @("parsed", "parse_rejected")) {
        $errors += "parse_decision is invalid"
    }
    if ($Prediction.analysis_decision -isnot [string] -or
        $Prediction.analysis_decision -notin @("resolve", "needs_clarification", "insufficient")) {
        $errors += "analysis_decision is invalid"
    }
    if ($null -eq $Prediction.items -or $Prediction.items -isnot [System.Array]) {
        $errors += "items must be an array"
    }
    $items = @($Prediction.items)
    if ($Prediction.parse_decision -eq "parsed" -and $items.Count -eq 0) {
        $errors += "parsed predictions require at least one item"
    }
    if ($Prediction.parse_decision -eq "parse_rejected" -and
        ($items.Count -ne 0 -or $Prediction.analysis_decision -ne "insufficient")) {
        $errors += "parse_rejected predictions require zero items and analysis insufficient"
    }
    $dimensionPresent = $null -ne $Prediction.PSObject.Properties["analysis_clarification_dimension"]
    if ($dimensionPresent -and $null -ne $Prediction.analysis_clarification_dimension -and
        $Prediction.analysis_clarification_dimension -isnot [string]) {
        $errors += "analysis_clarification_dimension must be a string or null"
    }
    if ($Prediction.analysis_decision -eq "needs_clarification" -and
        ($null -eq $Prediction.analysis_clarification_dimension -or
         $Prediction.analysis_clarification_dimension -isnot [string] -or
         [string]::IsNullOrWhiteSpace($Prediction.analysis_clarification_dimension))) {
        $errors += "needs_clarification requires a non-empty dimension"
    }
    if ($Prediction.analysis_decision -ne "needs_clarification" -and
        $null -ne $Prediction.analysis_clarification_dimension) {
        $errors += "non-clarification analysis decisions require a null or absent dimension"
    }
    foreach ($item in $items) {
        if (-not (Test-AllowedProperties $item @("food_phrase", "quantity", "unit", "preparation", "modifiers", "negated"))) {
            $errors += "item has unexpected properties"
            continue
        }
        if ($item.food_phrase -isnot [string] -or [string]::IsNullOrWhiteSpace($item.food_phrase)) {
            $errors += "item food_phrase must be a non-empty string"
        }
        if ($null -ne $item.quantity -and $item.quantity -isnot [string]) {
            $errors += "item quantity must be a string or null"
        }
        if ($null -ne $item.unit -and $item.unit -isnot [string]) {
            $errors += "item unit must be a string or null"
        }
        foreach ($listName in @("preparation", "modifiers")) {
            $list = @($item.$listName)
            if ($null -eq $item.$listName -or $item.$listName -isnot [System.Array]) {
                $errors += "item $listName must be an array"
            } elseif (@($list | Where-Object { $_ -isnot [string] }).Count -gt 0) {
                $errors += "item $listName must contain strings"
            }
        }
        if ($item.negated -isnot [bool]) {
            $errors += "item negated must be boolean"
        }
    }
    if ($null -eq $Prediction.safety_flags -or $Prediction.safety_flags -isnot [System.Array]) {
        $errors += "safety_flags must be an array"
    } elseif ($null -eq (Get-NormalizedSafetyFlags @($Prediction.safety_flags))) {
        $errors += "safety_flags must be unique non-empty strings"
    }

    return [pscustomobject]@{
        valid = $errors.Count -eq 0
        errors = $errors
        normalized_safety_flags = if ($null -ne $Prediction.safety_flags) { Get-NormalizedSafetyFlags @($Prediction.safety_flags) } else { $null }
    }
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

function Convert-Comparable {
    param($Value)
    return ($Value | ConvertTo-Json -Depth 20 -Compress)
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
$invalid = 0
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
    $expectedPhrases = Get-PhraseCounts @($expected.expected_parse.items)
    $caseExpectedMentions = @($expected.expected_parse.items).Count
    $expectedMentions += $caseExpectedMentions
    foreach ($tag in @($expected.tags)) {
        $tagKey = [string]$tag
        if (-not $sliceData.ContainsKey($tagKey)) {
            $sliceData[$tagKey] = [ordered]@{
                case_count = 0
                schema_valid = 0
                parse_correct = 0
                analysis_correct = 0
                expected_mentions = 0
                true_positives = 0
            }
        }
        $sliceData[$tagKey].case_count++
        $sliceData[$tagKey].expected_mentions += $caseExpectedMentions
    }

    if (-not $predictionMap.ContainsKey($id)) {
        $missing++
        continue
    }
    $prediction = $predictionMap[$id]
    $validation = Test-PredictionSchema $prediction
    if (-not $validation.valid) {
        $invalid++
        continue
    }
    $schemaValid++

    $expectedParse = [string]$expected.expected_parse_decision
    $expectedAnalysis = [string]$expected.expected_analysis_decision
    if ([string]$prediction.parse_decision -eq $expectedParse) { $parseCorrect++ }
    $expectedDimension = [string]$expected.expected_analysis_clarification_dimension
    $predictionDimension = [string]$prediction.analysis_clarification_dimension
    $analysisMatches = [string]$prediction.analysis_decision -eq $expectedAnalysis -and
        ($expectedAnalysis -ne "needs_clarification" -or $predictionDimension -eq $expectedDimension)
    if ($analysisMatches) { $analysisCorrect++ }
    $expectedSafety = @($expected.expected_parse.safety_flags | ForEach-Object { Normalize-Text ([string]$_) } | Sort-Object)
    if ((Convert-Comparable $validation.normalized_safety_flags) -eq (Convert-Comparable $expectedSafety)) {
        $safetyCorrect++
    }

    $predictionPhrases = Get-PhraseCounts @($prediction.items)
    $caseTruePositives = Get-IntersectionCount $expectedPhrases $predictionPhrases
    $truePositives += $caseTruePositives
    $predictedMentions += @($prediction.items).Count
    if ([string]$prediction.analysis_decision -eq "resolve" -and $expectedAnalysis -ne "resolve") {
        $falseResolveCount++
    }

    foreach ($tag in @($expected.tags)) {
        $tagKey = [string]$tag
        $sliceData[$tagKey].schema_valid++
        if ([string]$prediction.parse_decision -eq $expectedParse) { $sliceData[$tagKey].parse_correct++ }
        if ($analysisMatches) { $sliceData[$tagKey].analysis_correct++ }
        $sliceData[$tagKey].true_positives += $caseTruePositives
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
    prediction_schema = [string]$manifest.annotation.prediction_schema
    schema_validator = "deterministic_equivalent_to_versioned_schema"
    split = $SplitName
    expected_file_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $ExpectedPath).Hash.ToLowerInvariant()
    predictions_file_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $PredictionsPath).Hash.ToLowerInvariant()
    counts = [ordered]@{
        expected = $total
        predictions = $predictions.Count
        schema_valid = $schemaValid
        schema_invalid = $invalid
        missing_predictions = $missing
        unexpected_predictions = $unexpected
        expected_mentions = $expectedMentions
        predicted_mentions_from_valid_predictions = $predictedMentions
        matched_mentions = $truePositives
    }
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
    $sliceMentionRecall = if ($data.expected_mentions -eq 0) { 0 } else { $data.true_positives / $data.expected_mentions }
    $report.slices[$tag] = [ordered]@{
        case_count = $data.case_count
        schema_valid_rate = [math]::Round($data.schema_valid / $data.case_count, 6)
        parse_decision_accuracy = [math]::Round($data.parse_correct / $data.case_count, 6)
        analysis_decision_accuracy = [math]::Round($data.analysis_correct / $data.case_count, 6)
        expected_mentions = $data.expected_mentions
        mention_recall = [math]::Round($sliceMentionRecall, 6)
    }
}

$json = $report | ConvertTo-Json -Depth 30
if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
    $json | Set-Content -LiteralPath $OutputPath -Encoding utf8
}
Write-Output $json
