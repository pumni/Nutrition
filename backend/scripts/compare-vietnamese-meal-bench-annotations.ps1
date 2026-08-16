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

function Read-Packet {
    param([string]$Path, [string]$ExpectedSlot)
    $packet = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
    if ($packet.packet_type -ne "independent_parse_annotation") {
        throw "$Path is not an independent parse annotation packet"
    }
    if ($packet.annotator_slot -ne $ExpectedSlot) {
        throw "$Path must be the $ExpectedSlot packet"
    }
    return $packet
}

function Convert-Comparable {
    param($Value)
    return ($Value | ConvertTo-Json -Depth 20 -Compress)
}

$packetA = Read-Packet $AnnotatorAPath "independent-a"
$packetB = Read-Packet $AnnotatorBPath "independent-b"
if ($packetA.version -ne $packetB.version -or
    $packetA.source_case_file_sha256 -ne $packetB.source_case_file_sha256) {
    throw "annotation packets must use the same benchmark version and source case hash"
}

$casesA = @($packetA.cases)
$casesB = @($packetB.cases)
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
    $differences = @()
    foreach ($field in @("parse_decision", "items", "safety_flags")) {
        if ((Convert-Comparable $caseA.annotation.$field) -ne (Convert-Comparable $caseB.annotation.$field)) {
            $differences += $field
        }
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
    version = [string]$packetA.version
    source_case_file_sha256 = [string]$packetA.source_case_file_sha256
    independent_annotator_slots = @("independent-a", "independent-b")
    case_count = $casesA.Count
    agreement_count = $agreementCount
    disagreement_count = $disagreements.Count
    disagreement_rate = if ($casesA.Count -eq 0) { 0 } else { [math]::Round($disagreements.Count / $casesA.Count, 6) }
    domain_adjudication_required = $true
    human_adjudication_complete = $false
    status = if ($disagreements.Count -gt 0) { "needs_domain_adjudication" } else { "awaiting_domain_adjudicator" }
    disagreements = $disagreements
    note = "This report compares independent annotations only; it does not approve gold labels or analysis expectations."
}

$report | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Output ([ordered]@{
    output = (Resolve-Path -LiteralPath $OutputPath).Path
    case_count = $casesA.Count
    disagreement_count = $disagreements.Count
    status = $report.status
} | ConvertTo-Json -Compress)
