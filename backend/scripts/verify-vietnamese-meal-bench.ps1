param(
    [string]$RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot ".." )).Path
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$benchRoot = Join-Path $RepositoryRoot "fixtures\vietnamese-meal-bench"
$manifestPath = Join-Path $benchRoot "manifest.json"
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$errors = [System.Collections.Generic.List[string]]::new()
$caseIds = [System.Collections.Generic.HashSet[string]]::new()
$tagCounts = @{}
$decisionCounts = @{}
$annotationCounts = @{}
$splitReports = [ordered]@{}
$loadedCaseCount = 0

if ($manifest.benchmark -ne "VietnameseMealBench") {
    $errors.Add("manifest benchmark must be VietnameseMealBench")
}
if ($manifest.status -ne "development-only") {
    $errors.Add("benchmark release must remain development-only")
}

foreach ($splitName in @("development", "public_test")) {
    $splitProperty = $manifest.splits.PSObject.Properties[$splitName]
    if ($null -eq $splitProperty) {
        $errors.Add("manifest is missing split $splitName")
        continue
    }
    $split = $splitProperty.Value
    if ([string]::IsNullOrWhiteSpace([string]$split.case_file)) {
        $errors.Add("split $splitName must define a case_file")
        continue
    }
    $casePath = Join-Path $benchRoot ([string]$split.case_file)
    if (-not (Test-Path -LiteralPath $casePath -PathType Leaf)) {
        $errors.Add("split $splitName case file is missing: $($split.case_file)")
        continue
    }
    $cases = @(Get-Content -Raw -LiteralPath $casePath | ConvertFrom-Json)
    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $casePath).Hash.ToLowerInvariant()
    $splitReports[$splitName] = [ordered]@{
        case_file = [string]$split.case_file
        case_count = $cases.Count
        declared_case_count = [int]$split.case_count
        file_sha256 = $actualHash
    }
    if ($cases.Count -ne [int]$split.case_count) {
        $errors.Add("split $splitName declares $($split.case_count) cases but contains $($cases.Count)")
    }

    foreach ($case in $cases) {
        $loadedCaseCount++
        $sampleId = [string]$case.sample_id
        if ([string]::IsNullOrWhiteSpace($sampleId)) {
            $errors.Add("split $splitName contains a case without sample_id")
        } elseif (-not $caseIds.Add($sampleId)) {
            $errors.Add("duplicate sample_id: $sampleId")
        }
        if ([string]$case.locale -ne "vi-VN") {
            $errors.Add("case $sampleId must use locale vi-VN")
        }
        if ([string]$case.expected_decision -notin @("resolve", "needs_clarification", "insufficient")) {
            $errors.Add("case $sampleId has an invalid expected_decision")
        }
        if ($null -eq $case.expected_parse) {
            $errors.Add("case $sampleId is missing expected_parse")
            continue
        }
        $items = @($case.expected_parse.items)
        if ([string]$case.expected_decision -eq "insufficient" -and $items.Count -gt 0) {
            $errors.Add("insufficient case $sampleId must not contain resolved items")
        }
        if ([string]$case.expected_decision -eq "needs_clarification" -and
            [string]::IsNullOrWhiteSpace([string]$case.expected_parse.clarification_dimension)) {
            $errors.Add("clarification case $sampleId must declare clarification_dimension")
        }
        foreach ($item in $items) {
            if ([string]::IsNullOrWhiteSpace([string]$item.food_phrase)) {
                $errors.Add("case $sampleId contains an item without food_phrase")
            }
            if ($null -eq $item.preparation -or $null -eq $item.modifiers) {
                $errors.Add("case $sampleId must declare preparation and modifiers arrays")
            }
        }
        foreach ($tag in @($case.tags)) {
            $tagKey = [string]$tag
            if (-not $tagCounts.ContainsKey($tagKey)) { $tagCounts[$tagKey] = 0 }
            $tagCounts[$tagKey]++
        }
        $decisionKey = [string]$case.expected_decision
        if (-not $decisionCounts.ContainsKey($decisionKey)) { $decisionCounts[$decisionKey] = 0 }
        $decisionCounts[$decisionKey]++
        $annotationKey = [string]$case.adjudication_status
        if ([string]::IsNullOrWhiteSpace($annotationKey)) {
            $errors.Add("case $sampleId is missing adjudication_status")
        } else {
            if (-not $annotationCounts.ContainsKey($annotationKey)) { $annotationCounts[$annotationKey] = 0 }
            $annotationCounts[$annotationKey]++
        }
    }
}

foreach ($splitName in @("sealed_test", "challenge")) {
    $splitProperty = $manifest.splits.PSObject.Properties[$splitName]
    if ($null -eq $splitProperty) {
        $errors.Add("manifest is missing split $splitName")
        continue
    }
    $split = $splitProperty.Value
    if ($split.answers_in_repo -ne $false) {
        $errors.Add("split $splitName must keep answers_in_repo=false")
    }
    if ($null -ne $split.case_file) {
        $errors.Add("split $splitName must not expose a case_file in this repository")
    }
}

function Convert-CountsToOrderedMap {
    param([hashtable]$Counts)
    $ordered = [ordered]@{}
    foreach ($entry in ($Counts.GetEnumerator() | Sort-Object Name)) {
        $ordered[$entry.Name] = $entry.Value
    }
    return $ordered
}

$report = [ordered]@{
    benchmark = [string]$manifest.benchmark
    version = [string]$manifest.version
    source_status = [string]$manifest.status
    loaded_case_count = $loadedCaseCount
    splits = $splitReports
    coverage = [ordered]@{
        tags = Convert-CountsToOrderedMap $tagCounts
        expected_decisions = Convert-CountsToOrderedMap $decisionCounts
        annotation_status = Convert-CountsToOrderedMap $annotationCounts
    }
    release = [ordered]@{
        status = if ($errors.Count -eq 0 -and $annotationCounts.ContainsKey("pending_human_review")) { "pending_human_review" } else { "blocked" }
        sealed_answers_loaded = $false
        production_gate = "not_eligible"
        human_adjudication_required = [bool]$manifest.release_gates.human_adjudication_required
    }
    errors = @($errors | Sort-Object)
}

$report | ConvertTo-Json -Depth 20
if ($errors.Count -gt 0) {
    throw "VietnameseMealBench validation failed with $($errors.Count) error(s)"
}
