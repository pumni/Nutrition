param(
    [Parameter(Mandatory = $true)]
    [string]$AnnotatorAPath,
    [Parameter(Mandatory = $true)]
    [string]$AnnotatorBPath,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

function Normalize-Text {
    param([AllowNull()][string]$Value)
    if ($null -eq $Value) { return "" }
    return [regex]::Replace($Value.Trim().ToLowerInvariant(), "\s+", " ")
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

function Test-CompletedAnnotation {
    param($Case)
    $errors = @()
    if (-not (Test-AllowedProperties $Case @("sample_id", "text", "locale", "annotation"))) {
        $errors += "case has unexpected properties"
    }
    if ($Case.sample_id -isnot [string] -or [string]::IsNullOrWhiteSpace($Case.sample_id)) {
        $errors += "sample_id must be a non-empty string"
    }
    if ($Case.text -isnot [string] -or [string]::IsNullOrWhiteSpace($Case.text)) {
        $errors += "text must be a non-empty string"
    }
    if ($Case.locale -isnot [string] -or $Case.locale -cne "vi-VN") {
        $errors += "locale must be vi-VN"
    }

    $annotation = $Case.annotation
    if (-not (Test-AllowedProperties $annotation @("parse_decision", "items", "safety_flags"))) {
        $errors += "annotation has unexpected properties"
    }
    if ($annotation.parse_decision -isnot [string] -or
        $annotation.parse_decision -notin @("parsed", "parse_rejected")) {
        $errors += "parse_decision is invalid or incomplete"
    }
    if ($null -eq $annotation.items) {
        $errors += "items is required"
    }
    $items = @($annotation.items)
    if ($annotation.parse_decision -eq "parsed" -and $items.Count -eq 0) {
        $errors += "parsed annotation must contain at least one item"
    }
    if ($annotation.parse_decision -eq "parse_rejected" -and $items.Count -ne 0) {
        $errors += "parse_rejected annotation must contain zero items"
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
    if ($null -eq $annotation.safety_flags -or $annotation.safety_flags -isnot [System.Array]) {
        $errors += "safety_flags must be an array"
        $normalizedSafetyFlags = $null
    } else {
        $normalizedSafetyFlags = Get-NormalizedSafetyFlags @($annotation.safety_flags)
        if ($null -eq $normalizedSafetyFlags) {
            $errors += "safety_flags must be unique non-empty strings"
        }
    }

    return [pscustomobject]@{
        valid = $errors.Count -eq 0
        errors = $errors
        normalized_safety_flags = $normalizedSafetyFlags
    }
}

function Convert-Comparable {
    param($Value)
    return ($Value | ConvertTo-Json -Depth 20 -Compress)
}

function Read-Packet {
    param([string]$Path, [string]$ExpectedSlot)
    $packet = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
    if (-not (Test-AllowedProperties $packet @("benchmark", "version", "packet_type", "annotator_slot", "source_case_file_sha256", "guideline", "cases"))) {
        throw "$Path has unexpected packet properties"
    }
    if ($packet.benchmark -ne "VietnameseMealBench" -or
        $packet.packet_type -ne "independent_parse_annotation" -or
        $packet.annotator_slot -ne $ExpectedSlot -or
        [string]::IsNullOrWhiteSpace([string]$packet.version) -or
        [string]$packet.source_case_file_sha256 -notmatch "^[0-9a-fA-F]{64}$" -or
        $packet.cases -isnot [System.Array]) {
        throw "$Path has an invalid packet header"
    }
    $validation = @{}
    foreach ($case in @($packet.cases)) {
        $result = Test-CompletedAnnotation $case
        $sampleId = [string]$case.sample_id
        if (-not $result.valid) {
            throw ("{0} contains an incomplete or invalid annotation for sample_id {1}: {2}" -f $Path, $sampleId, ($result.errors -join "; "))
        }
        if ($validation.ContainsKey($sampleId)) {
            throw "$Path contains duplicate sample_id $sampleId"
        }
        $validation[$sampleId] = $result
    }
    return [pscustomobject]@{
        packet = $packet
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
        validation = $validation
    }
}

$packetA = Read-Packet $AnnotatorAPath "independent-a"
$packetB = Read-Packet $AnnotatorBPath "independent-b"
if ($packetA.packet.version -ne $packetB.packet.version -or
    $packetA.packet.source_case_file_sha256 -ne $packetB.packet.source_case_file_sha256) {
    throw "annotation packets must use the same benchmark version and source case hash"
}

$casesA = @($packetA.packet.cases)
$casesB = @($packetB.packet.cases)
if ($casesA.Count -ne $casesB.Count) {
    throw "annotation packets must contain the same number of cases"
}

$mapB = @{}
foreach ($case in $casesB) {
    if ($mapB.ContainsKey([string]$case.sample_id)) {
        throw "annotator-b contains a duplicate sample_id"
    }
    $mapB[[string]$case.sample_id] = $case
}
$mapA = @{}
foreach ($case in $casesA) {
    if ($mapA.ContainsKey([string]$case.sample_id)) {
        throw "annotator-a contains a duplicate sample_id"
    }
    $mapA[[string]$case.sample_id] = $case
}
foreach ($sampleId in $mapB.Keys) {
    if (-not $mapA.ContainsKey($sampleId)) {
        throw "annotator-a is missing sample_id $sampleId"
    }
}

$disagreements = @()
$agreementCount = 0
foreach ($caseA in $casesA) {
    $sampleId = [string]$caseA.sample_id
    if (-not $mapB.ContainsKey($sampleId)) {
        throw "annotator-b is missing sample_id $sampleId"
    }
    $caseB = $mapB[$sampleId]
    if ([string]$caseA.text -cne [string]$caseB.text) {
        throw "text differs for sample_id $sampleId"
    }
    if ([string]$caseA.locale -cne [string]$caseB.locale) {
        throw "locale differs for sample_id $sampleId"
    }
    $differences = @()
    if ((Convert-Comparable $caseA.annotation.parse_decision) -ne
        (Convert-Comparable $caseB.annotation.parse_decision)) {
        $differences += "parse_decision"
    }
    if ((Convert-Comparable $caseA.annotation.items) -ne
        (Convert-Comparable $caseB.annotation.items)) {
        $differences += "items"
    }
    $safetyA = $packetA.validation[$sampleId].normalized_safety_flags
    $safetyB = $packetB.validation[$sampleId].normalized_safety_flags
    if ((Convert-Comparable $safetyA) -ne (Convert-Comparable $safetyB)) {
        $differences += "safety_flags"
    }
    if ($differences.Count -eq 0) {
        $agreementCount++
    } else {
        $disagreements += [ordered]@{
            sample_id = $sampleId
            disagreement_fields = $differences
            annotator_a = $caseA.annotation
            annotator_b = $caseB.annotation
        }
    }
}

$report = [ordered]@{
    benchmark = "VietnameseMealBench"
    version = [string]$packetA.packet.version
    source_case_file_sha256 = [string]$packetA.packet.source_case_file_sha256
    annotator_a_packet_sha256 = $packetA.sha256
    annotator_b_packet_sha256 = $packetB.sha256
    independent_annotator_slots = @("independent-a", "independent-b")
    case_count = $casesA.Count
    agreement_count = $agreementCount
    disagreement_count = $disagreements.Count
    disagreement_rate = if ($casesA.Count -eq 0) { 0 } else { [math]::Round($disagreements.Count / $casesA.Count, 6) }
    domain_adjudication_required = $true
    human_adjudication_complete = $false
    status = if ($disagreements.Count -gt 0) { "needs_domain_adjudication" } else { "awaiting_domain_adjudicator" }
    disagreements = $disagreements
    note = "This report compares completed independent annotations only; it does not approve gold labels or analysis expectations."
}

$report | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Output ([ordered]@{
    output = (Resolve-Path -LiteralPath $OutputPath).Path
    case_count = $casesA.Count
    disagreement_count = $disagreements.Count
    annotator_a_packet_sha256 = $packetA.sha256
    annotator_b_packet_sha256 = $packetB.sha256
    status = $report.status
} | ConvertTo-Json -Compress)
