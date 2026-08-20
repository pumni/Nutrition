[CmdletBinding()]
param(
    [string]$RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot ".." )).Path,
    [string]$CasesPath,
    [string]$ManifestPath,
    [string]$CatalogSeedPath,
    [string]$OutputPath
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$RepositoryRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path

function Resolve-RepositoryFile {
    param([string]$Path, [string]$DefaultRelativePath)
    $candidate = if ([string]::IsNullOrWhiteSpace($Path)) {
        Join-Path $RepositoryRoot $DefaultRelativePath
    } elseif ([IO.Path]::IsPathRooted($Path)) {
        $Path
    } else {
        Join-Path $RepositoryRoot $Path
    }
    return (Resolve-Path -LiteralPath $candidate -ErrorAction Stop).Path
}

function Normalize-RepositoryPath {
    param([string]$Path)
    return $Path.Substring($RepositoryRoot.Length + 1).Replace('\', '/')
}

function Normalize-VietnameseSearchKey {
    param([string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return "" }
    $normalized = $Value.Normalize([Text.NormalizationForm]::FormC).ToLowerInvariant()
    return (($normalized -split '\s+') -join ' ').Trim()
}

function Get-Sha256 {
    param([string]$Path)
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-SqlInsertRows {
    param([string]$Sql, [string]$TableName)
    $tablePattern = [regex]::Escape($TableName)
    $pattern = "(?is)INSERT\s+INTO\s+$tablePattern\s*\(.*?\)\s*VALUES\s*(?<rows>.*?)\)\s+ON\s+CONFLICT"
    $match = [regex]::Match($Sql, $pattern)
    if (-not $match.Success) { throw "Could not locate SQL insert block for $TableName" }
    return $match.Groups["rows"].Value
}

function Add-Count {
    param([hashtable]$Counts, [string]$Key)
    if (-not $Counts.ContainsKey($Key)) { $Counts[$Key] = 0 }
    $Counts[$Key]++
}

function Convert-CountsToOrderedMap {
    param([hashtable]$Counts)
    $ordered = [ordered]@{}
    foreach ($key in ($Counts.Keys | Sort-Object)) { $ordered[$key] = $Counts[$key] }
    return $ordered
}

function Get-UniqueStrings {
    param([object[]]$Values)
    return @($Values | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } | ForEach-Object { [string]$_ } | Select-Object -Unique)
}

$casesFile = Resolve-RepositoryFile $CasesPath "fixtures/vietnamese-meal-bench/public-test-cases.json"
$manifestFile = Resolve-RepositoryFile $ManifestPath "fixtures/vietnamese-meal-bench/manifest.json"
$seedFile = Resolve-RepositoryFile $CatalogSeedPath "seeds/0001_foundation_fixture.sql"

$cases = @(Get-Content -Raw -LiteralPath $casesFile | ConvertFrom-Json)
$manifest = Get-Content -Raw -LiteralPath $manifestFile | ConvertFrom-Json
$seedSql = Get-Content -Raw -LiteralPath $seedFile

$foodNameRows = Get-SqlInsertRows $seedSql "catalog.food_name"
$foodNames = @()
$foodNamePattern = "(?is)\(\s*'(?<id>[^']+)'\s*,\s*'(?<food_id>[^']+)'\s*,\s*'(?<locale>[^']+)'\s*,\s*'(?<name>[^']+)'\s*,\s*'(?<normalized>[^']+)'\s*,\s*'(?<no_diacritics>[^']+)'\s*,\s*'(?<name_type>[^']+)'\s*,\s*(?<curated>true|false)\s*,"
foreach ($match in [regex]::Matches($foodNameRows, $foodNamePattern)) {
    if ($match.Groups["locale"].Value -ne "vi-VN") { continue }
    $foodNames += [pscustomobject][ordered]@{
        id = $match.Groups["id"].Value
        food_id = $match.Groups["food_id"].Value
        locale = $match.Groups["locale"].Value
        name = $match.Groups["name"].Value
        normalized_name = Normalize-VietnameseSearchKey $match.Groups["normalized"].Value
        name_type = $match.Groups["name_type"].Value
        is_curated = ($match.Groups["curated"].Value -eq "true")
    }
}
if ($foodNames.Count -eq 0) { throw "No vi-VN catalog names were parsed from $seedFile" }

$measureRows = Get-SqlInsertRows $seedSql "composition.measure_unit"
$measureUnits = @{}
$measurePattern = "(?is)\(\s*'(?<id>[^']+)'\s*,\s*'(?<code>[^']+)'\s*,\s*'(?<dimension>[^']+)'\s*,\s*'(?<label>[^']+)'\s*,\s*'(?<aliases>\[[^\]]*\])'"
foreach ($match in [regex]::Matches($measureRows, $measurePattern)) {
    $measureUnits[$match.Groups["id"].Value] = [pscustomobject][ordered]@{
        id = $match.Groups["id"].Value
        code = Normalize-VietnameseSearchKey $match.Groups["code"].Value
        label = $match.Groups["label"].Value
    }
}

$foodById = @{}
foreach ($foodName in $foodNames) {
    if (-not $foodById.ContainsKey($foodName.food_id)) { $foodById[$foodName.food_id] = $foodName.name }
}
$portionRows = Get-SqlInsertRows $seedSql "composition.portion_observation"
$portionUnitsByFood = @{}
$portionPattern = "(?is)\(\s*'(?<id>[^']+)'\s*,\s*'(?<food_id>[^']+)'\s*,\s*'(?<measure_id>[^']+)'\s*,"
foreach ($match in [regex]::Matches($portionRows, $portionPattern)) {
    $foodId = $match.Groups["food_id"].Value
    $measureId = $match.Groups["measure_id"].Value
    if (-not $foodById.ContainsKey($foodId) -or -not $measureUnits.ContainsKey($measureId)) { continue }
    if (-not $portionUnitsByFood.ContainsKey($foodId)) { $portionUnitsByFood[$foodId] = @() }
    $portionUnitsByFood[$foodId] += $measureUnits[$measureId]
}

$knownNames = @($foodNames | Where-Object is_curated | Sort-Object normalized_name)
$itemResults = @()
$caseResults = @()
$primaryCounts = @{}
$dimensionCounts = @{}
$caseCounts = @{}

foreach ($case in $cases) {
    $items = @($case.expected_parse.items)
    $caseItemResults = @()
    foreach ($item in $items) {
        $phrase = [string]$item.food_phrase
        $normalizedPhrase = Normalize-VietnameseSearchKey $phrase
        $unit = if ($null -eq $item.unit) { "" } else { Normalize-VietnameseSearchKey ([string]$item.unit) }
        $preparations = @(Get-UniqueStrings @($item.preparation))
        $preparationKeys = @($preparations | ForEach-Object { Normalize-VietnameseSearchKey $_ })
        $tags = @(Get-UniqueStrings @($case.tags))
        $expectedDimension = [string]$case.expected_analysis_clarification_dimension
        $exact = @($knownNames | Where-Object { $_.normalized_name -eq $normalizedPhrase })
        $phraseTokens = @($normalizedPhrase -split ' ' | Where-Object { $_ })
        $aliasCandidates = @($knownNames | Where-Object {
            $candidateTokens = @($_.normalized_name -split ' ' | Where-Object { $_ })
            $phraseTokens.Count -gt 0 -and
            @($phraseTokens | Where-Object { $_ -notin $candidateTokens }).Count -eq 0 -and
            $candidateTokens.Count -gt $phraseTokens.Count
        })

        $preparationCandidates = @()
        if ($preparationKeys.Count -gt 0) {
            $baseTokens = @($phraseTokens | Where-Object { $_ -notin $preparationKeys })
            $preparationCandidates = @($knownNames | Where-Object {
                $candidateTokens = @($_.normalized_name -split ' ' | Where-Object { $_ })
                $baseTokens.Count -gt 0 -and
                @($baseTokens | Where-Object { $_ -notin $candidateTokens }).Count -eq 0 -and
                @($preparationKeys | Where-Object { $_ -notin $candidateTokens }).Count -gt 0
            })
        }

        $portionUnits = @()
        if ($exact.Count -gt 0 -and $portionUnitsByFood.ContainsKey($exact[0].food_id)) {
            $portionUnits = @($portionUnitsByFood[$exact[0].food_id] | ForEach-Object { $_.code; $_.label } | Select-Object -Unique)
        }
        $unitSupported = $unit -eq "" -or $unit -eq "g" -or $unit -in $portionUnits
        $primary = ""
        $reason = ""
        $candidateNames = @()
        if ($exact.Count -gt 0 -and $unitSupported) {
            $primary = "resolvable_exact_identity"
            $reason = "normalized vi-VN food phrase has an exact curated foundation name and the requested mass/portion unit is represented"
        } elseif ($exact.Count -gt 0) {
            $primary = "portion_evidence_needed"
            $reason = "exact identity exists, but the requested non-gram unit has no seed portion observation for that food"
            $candidateNames = @($exact | ForEach-Object name)
        } elseif ($tags -contains "composite-dish" -or $expectedDimension -eq "recipe") {
            $primary = "recipe_evidence_needed"
            $reason = "benchmark metadata marks a composite or recipe-dependent food, and no exact catalog identity exists"
        } elseif ($aliasCandidates.Count -gt 0) {
            $primary = "missing_vi_vn_alias"
            $reason = "phrase is a strict token subset of an existing curated name; this is only a candidate mapping and is not approved"
            $candidateNames = @($aliasCandidates | ForEach-Object name)
        } elseif ($preparationCandidates.Count -gt 0) {
            $primary = "preparation_mismatch"
            $reason = "food tokens overlap a catalog identity but the observed preparation is not represented by that identity"
            $candidateNames = @($preparationCandidates | ForEach-Object name)
        } else {
            $primary = "missing_identity"
            $reason = "no exact curated vi-VN identity or deterministic candidate mapping exists in the foundation seed"
        }

        $dimensions = @($primary)
        if (($null -eq $item.quantity -or [string]::IsNullOrWhiteSpace([string]$item.quantity)) -or
            ($unit -ne "" -and $unit -ne "g" -and -not $unitSupported)) {
            $dimensions += "portion_evidence_needed"
        }
        $dimensions = @($dimensions | Select-Object -Unique)
        foreach ($dimension in $dimensions) { Add-Count $dimensionCounts $dimension }
        Add-Count $primaryCounts $primary

        $result = [pscustomobject][ordered]@{
            sample_id = [string]$case.sample_id
            food_phrase = $phrase
            normalized_food_phrase = $normalizedPhrase
            quantity = if ($null -eq $item.quantity) { $null } else { [string]$item.quantity }
            unit = if ([string]::IsNullOrWhiteSpace($unit)) { $null } else { $unit }
            preparation = $preparations
            benchmark_tags = $tags
            benchmark_expected_analysis_dimension = if ([string]::IsNullOrWhiteSpace($expectedDimension)) { $null } else { $expectedDimension }
            primary_gap_class = $primary
            gap_dimensions = $dimensions
            candidate_catalog_names = $candidateNames
            exact_catalog_name = if ($exact.Count -gt 0) { [string]$exact[0].name } else { $null }
            available_portion_units = $portionUnits
            classification_reason = $reason
            domain_approval_status = "not_approved_by_this_analysis"
        }
        $itemResults += $result
        $caseItemResults += $result
    }

    $caseClass = if ([string]$case.expected_parse_decision -eq "parse_rejected" -or $caseItemResults.Count -eq 0) {
        "intentionally_insufficient_or_unknown"
    } elseif (@($caseItemResults | Where-Object primary_gap_class -eq "resolvable_exact_identity").Count -eq $caseItemResults.Count) {
        "resolvable_exact_identity"
    } elseif (@($caseItemResults | Where-Object primary_gap_class -eq "recipe_evidence_needed").Count -gt 0) {
        "recipe_evidence_needed"
    } elseif (@($caseItemResults | Where-Object primary_gap_class -eq "portion_evidence_needed").Count -gt 0) {
        "portion_evidence_needed"
    } elseif (@($caseItemResults | Where-Object primary_gap_class -eq "missing_vi_vn_alias").Count -gt 0) {
        "missing_vi_vn_alias"
    } else {
        "missing_identity"
    }
    Add-Count $caseCounts $caseClass
    $caseResults += [pscustomobject][ordered]@{
        sample_id = [string]$case.sample_id
        parse_decision = [string]$case.expected_parse_decision
        benchmark_expected_analysis_decision = [string]$case.expected_analysis_decision
        benchmark_expected_analysis_dimension = if ([string]::IsNullOrWhiteSpace([string]$case.expected_analysis_clarification_dimension)) { $null } else { [string]$case.expected_analysis_clarification_dimension }
        item_count = $caseItemResults.Count
        case_gap_class = $caseClass
        domain_approval_status = "not_approved_by_this_analysis"
    }
}

$head = (& git -C $RepositoryRoot rev-parse HEAD 2>$null | Out-String).Trim()
$report = [ordered]@{
    schema_version = "1.0.0"
    analysis = "vietnamese-catalog-coverage"
    classification_policy_version = "catalog-coverage-0.1.0"
    repository_head = $head
    script_sha256 = Get-Sha256 $MyInvocation.MyCommand.Path
    source_refs = @(
        [ordered]@{ path = Normalize-RepositoryPath $manifestFile; sha256 = Get-Sha256 $manifestFile },
        [ordered]@{ path = Normalize-RepositoryPath $casesFile; sha256 = Get-Sha256 $casesFile },
        [ordered]@{ path = Normalize-RepositoryPath $seedFile; sha256 = Get-Sha256 $seedFile }
    )
    benchmark = [ordered]@{
        name = [string]$manifest.benchmark
        version = [string]$manifest.version
        status = [string]$manifest.status
        case_count = $cases.Count
        item_count = $itemResults.Count
    }
    catalog_model = [ordered]@{
        source = Normalize-RepositoryPath $seedFile
        status = "test_only_foundation_seed"
        locale = "vi-VN"
        exact_curated_names = @($knownNames | ForEach-Object name)
        portion_units_by_food = @($knownNames | ForEach-Object {
            $foodId = $_.food_id
            [ordered]@{
                food_name = $_.name
                units = if ($portionUnitsByFood.ContainsKey($foodId)) { @($portionUnitsByFood[$foodId] | ForEach-Object { $_.label } | Select-Object -Unique) } else { @() }
            }
        })
        production_eligible = $false
    }
    counts = [ordered]@{
        cases = $cases.Count
        parsed_items = $itemResults.Count
        primary_gap_classes = Convert-CountsToOrderedMap $primaryCounts
        gap_dimensions = Convert-CountsToOrderedMap $dimensionCounts
        case_gap_classes = Convert-CountsToOrderedMap $caseCounts
    }
    items = $itemResults
    cases_detail = $caseResults
    decision_boundary = [ordered]@{
        classifications_are = "deterministic comparison against the checked-in foundation seed and benchmark metadata"
        classifications_are_not = @("human curation", "approved aliases", "approved recipes", "production nutrition evidence", "portion release")
        next_step = "owner reviews the aggregate gaps and approves the smallest curation/evidence slice before any catalog mutation"
    }
}

$json = $report | ConvertTo-Json -Depth 50
if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
    $fullOutput = [IO.Path]::GetFullPath($OutputPath)
    $repositoryPrefix = $RepositoryRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if ($fullOutput.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "OutputPath must be outside RepositoryRoot so analysis artifacts cannot enter the repository"
    }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $fullOutput) | Out-Null
    $json | Set-Content -LiteralPath $fullOutput -Encoding utf8
}
Write-Output $json
