[CmdletBinding()]
param(
    [string]$RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string]$PackagePath,
    [string]$OutputPath,
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
$RepositoryRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$ContractPath = Join-Path $RepositoryRoot "docs/contracts/vietnamese-catalog-evidence-package-0.1.0.json"
$OwnerDecisionRef = "docs/OWNER_DECISIONS_V1.md#owner-be-002--initial-vietnamese-catalog-scope"

function Fail([string]$Message) { throw "[FAIL] $Message" }
function Has-Property($Object, [string]$Name) { $null -ne $Object.PSObject.Properties[$Name] }
function Exact-Properties($Object, [string[]]$Allowed, [string]$Context) {
    $unknown = @($Object.PSObject.Properties.Name | Where-Object { $_ -notin $Allowed })
    if ($unknown.Count -gt 0) { Fail "$Context contains unknown field(s): $($unknown -join ', ')" }
}
function Require-Properties($Object, [string[]]$Required, [string]$Context) {
    foreach ($name in $Required) { if (-not (Has-Property $Object $name)) { Fail "$Context is missing '$name'" } }
}
function Require-Array([object]$Value, [string]$Context) {
    if ($null -eq $Value -or $Value -isnot [System.Array]) { Fail "$Context must be an array" }
}
function NonEmpty([object]$Value, [string]$Context) {
    if ([string]::IsNullOrWhiteSpace([string]$Value)) { Fail "$Context must be non-empty" }
}
function Safe-Reference([string]$Value, [string]$Context) {
    NonEmpty $Value $Context
    if ($Value.Contains("`r") -or $Value.Contains("`n") -or $Value.Contains("..") -or $Value.StartsWith('/') -or $Value.StartsWith('\') -or $Value -match '^[A-Za-z]:[\\/]') { Fail "$Context is unsafe" }
}
function Reject-Prohibited-Source([string]$Value, [string]$Context) {
    $normalized = $Value.ToLowerInvariant() -replace '[^a-z0-9]+', ' '
    if ($normalized -match 'vietnam' -and $normalized -match 'fct|food composition table' -and $normalized -match '2017') {
        Fail "$Context references the prohibited Vietnam FCT 2017 source"
    }
}
function Identifier([string]$Value, [string]$Context) {
    if ($Value -notmatch '^[a-z0-9][a-z0-9._-]{2,127}$') { Fail "$Context must match the package identifier pattern" }
}
function Sha256([string]$Value, [string]$Context) {
    if ($Value -notmatch '^[0-9a-fA-F]{64}$') { Fail "$Context must be a SHA-256 digest" }
}
function Positive([object]$Value, [string]$Context) {
    if ($Value -isnot [int] -and $Value -isnot [long] -and $Value -isnot [double] -and $Value -isnot [decimal]) { Fail "$Context must be numeric" }
    if ([double]$Value -le 0 -or [double]::IsNaN([double]$Value) -or [double]::IsInfinity([double]$Value)) { Fail "$Context must be positive and finite" }
}
function Positive-Integer([object]$Value, [string]$Context) {
    Positive $Value $Context
    if ([double]$Value -ne [math]::Truncate([double]$Value)) { Fail "$Context must be an integer" }
}
function Json([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail "JSON file does not exist: $Path" }
    try { return Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json } catch { Fail "invalid JSON: $Path" }
}
function Validate-Provenance($Values, [string]$Context) {
    Require-Array $Values $Context
    $items = @($Values)
    if ($items.Count -eq 0) { Fail "$Context must be non-empty" }
    foreach ($item in $items) {
        Exact-Properties $item @('evidence_ref','sha256') $Context
        Require-Properties $item @('evidence_ref','sha256') $Context
        Safe-Reference ([string]$item.evidence_ref) "$Context evidence_ref"
        Reject-Prohibited-Source ([string]$item.evidence_ref) "$Context evidence_ref"
        Sha256 ([string]$item.sha256) "$Context sha256"
    }
}
function Validate-SourceRefs($Values, [string]$Context) {
    Require-Array $Values $Context
    $items = @($Values)
    if ($items.Count -eq 0) { Fail "$Context must be non-empty" }
    foreach ($item in $items) {
        Exact-Properties $item @('source_ref','sha256') $Context
        Require-Properties $item @('source_ref','sha256') $Context
        Safe-Reference ([string]$item.source_ref) "$Context source_ref"
        Reject-Prohibited-Source ([string]$item.source_ref) "$Context source_ref"
        Sha256 ([string]$item.sha256) "$Context sha256"
    }
}
function Validate-Review($Review, [string]$Context) {
    Exact-Properties $Review @('status','reviewer_ref','reviewer_role','decision_ref') $Context
    Require-Properties $Review @('status','reviewer_ref','reviewer_role','decision_ref') $Context
    if ([string]$Review.status -notin @('proposed','owner_approved','domain_reviewed')) { Fail "$Context status is invalid" }
    if ([string]$Review.reviewer_role -notin @('none','human_owner','human_domain_reviewer')) { Fail "$Context reviewer_role is invalid" }
    Safe-Reference ([string]$Review.reviewer_ref) "$Context reviewer_ref"
    if ([string]$Review.status -eq 'proposed' -and [string]$Review.reviewer_role -ne 'none') { Fail "$Context proposed records cannot claim a human reviewer" }
    if ([string]$Review.status -ne 'proposed' -and [string]$Review.reviewer_role -eq 'none') { Fail "$Context reviewed records require a reviewer role" }
    if ([string]$Review.decision_ref -ne $OwnerDecisionRef) { Fail "$Context decision_ref must reference OWNER-BE-002" }
}
function Validate-Common($Record, [string]$Context) {
    Require-Properties $Record @('record_id','record_kind','locale','review','provenance','release_status','production_eligible') $Context
    Identifier ([string]$Record.record_id) "$Context record_id"
    if ([string]$Record.locale -ne 'vi-VN') { Fail "$Context locale must be vi-VN" }
    if ([string]$Record.release_status -notin @('draft','staged')) { Fail "$Context release_status is invalid" }
    if ($Record.production_eligible -ne $false) { Fail "$Context production_eligible must be false" }
    Validate-Review $Record.review "$Context review"
    Validate-Provenance $Record.provenance "$Context provenance"
}
function Validate-Record($Record, [int]$Index) {
    $context = "record[$Index]"
    if (-not (Has-Property $Record 'record_kind')) { Fail "$context is missing record_kind" }
    Validate-Common $Record $context
    switch ([string]$Record.record_kind) {
        'identity' {
            Exact-Properties $Record @('record_id','record_kind','locale','canonical_food_id','canonical_name','review','provenance','release_status','production_eligible') $context
            Require-Properties $Record @('canonical_food_id','canonical_name') $context
            NonEmpty ([string]$Record.canonical_food_id) "$context canonical_food_id"
            NonEmpty ([string]$Record.canonical_name) "$context canonical_name"
        }
        'alias' {
            Exact-Properties $Record @('record_id','record_kind','locale','canonical_food_id','alias','review','provenance','release_status','production_eligible') $context
            Require-Properties $Record @('canonical_food_id','alias') $context
            NonEmpty ([string]$Record.canonical_food_id) "$context canonical_food_id"
            NonEmpty ([string]$Record.alias) "$context alias"
        }
        'recipe' {
            Exact-Properties $Record @('record_id','record_kind','locale','recipe_id','ingredients','cooked_yield','review','provenance','release_status','production_eligible') $context
            Require-Properties $Record @('recipe_id','ingredients','cooked_yield') $context
            NonEmpty ([string]$Record.recipe_id) "$context recipe_id"
            Require-Array $Record.ingredients "$context ingredients"
            $ingredients = @($Record.ingredients)
            if ($ingredients.Count -eq 0) { Fail "$context ingredients must be non-empty" }
            foreach ($ingredient in $ingredients) {
                Exact-Properties $ingredient @('food_id','quantity','unit') "$context ingredient"
                Require-Properties $ingredient @('food_id','quantity','unit') "$context ingredient"
                NonEmpty ([string]$ingredient.food_id) "$context ingredient food_id"
                Positive $ingredient.quantity "$context ingredient quantity"
                NonEmpty ([string]$ingredient.unit) "$context ingredient unit"
            }
            Exact-Properties $Record.cooked_yield @('quantity','unit') "$context cooked_yield"
            Require-Properties $Record.cooked_yield @('quantity','unit') "$context cooked_yield"
            Positive $Record.cooked_yield.quantity "$context cooked_yield quantity"
            NonEmpty ([string]$Record.cooked_yield.unit) "$context cooked_yield unit"
        }
        'portion' {
            Exact-Properties $Record @('record_id','record_kind','locale','food_id','preparation_state','measure','represented_quantity','study_id','protocol_version','independent_samples','estimate','review','provenance','release_status','production_eligible') $context
            Require-Properties $Record @('food_id','preparation_state','measure','represented_quantity','study_id','protocol_version','independent_samples','estimate') $context
            NonEmpty ([string]$Record.food_id) "$context food_id"
            NonEmpty ([string]$Record.preparation_state) "$context preparation_state"
            Exact-Properties $Record.measure @('class','code','context') "$context measure"
            Require-Properties $Record.measure @('class','code','context') "$context measure"
            NonEmpty ([string]$Record.measure.class) "$context measure class"
            NonEmpty ([string]$Record.measure.code) "$context measure code"
            NonEmpty ([string]$Record.measure.context) "$context measure context"
            Positive $Record.represented_quantity "$context represented_quantity"
            NonEmpty ([string]$Record.study_id) "$context study_id"
            NonEmpty ([string]$Record.protocol_version) "$context protocol_version"
            Require-Array $Record.independent_samples "$context independent_samples"
            $samples = @($Record.independent_samples)
            if ($samples.Count -eq 0) { Fail "$context independent_samples must be non-empty" }
            foreach ($sample in $samples) { Positive $sample "$context independent sample" }
            Exact-Properties $Record.estimate @('lower_gram_weight','gram_weight','upper_gram_weight','sample_count') "$context estimate"
            Require-Properties $Record.estimate @('lower_gram_weight','gram_weight','upper_gram_weight','sample_count') "$context estimate"
            Positive $Record.estimate.lower_gram_weight "$context lower_gram_weight"
            Positive $Record.estimate.gram_weight "$context gram_weight"
            Positive $Record.estimate.upper_gram_weight "$context upper_gram_weight"
            Positive-Integer $Record.estimate.sample_count "$context sample_count"
            if ([int64]$Record.estimate.sample_count -ne $samples.Count) { Fail "$context sample_count must equal independent_samples count" }
            if ([double]$Record.estimate.lower_gram_weight -gt [double]$Record.estimate.gram_weight -or [double]$Record.estimate.gram_weight -gt [double]$Record.estimate.upper_gram_weight) { Fail "$context must satisfy lower <= central <= upper" }
        }
        default { Fail "$context record_kind is unsupported" }
    }
}
function Validate-Package($Package) {
    Exact-Properties $Package @('schema_version','package_id','package_kind','owner_decision_ref','source_refs','release','records') 'package'
    Require-Properties $Package @('schema_version','package_id','package_kind','owner_decision_ref','source_refs','release','records') 'package'
    if ([string]$Package.schema_version -ne '0.1.0') { Fail 'package schema_version is invalid' }
    if ([string]$Package.package_kind -ne 'vietnamese-catalog-evidence') { Fail 'package_kind is invalid' }
    Identifier ([string]$Package.package_id) 'package_id'
    if ([string]$Package.owner_decision_ref -ne $OwnerDecisionRef) { Fail 'package must reference OWNER-BE-002' }
    Validate-SourceRefs $Package.source_refs 'package source_refs'
    Exact-Properties $Package.release @('release_id','release_version','status','production_eligible','activation_authorized') 'package release'
    Require-Properties $Package.release @('release_id','release_version','status','production_eligible','activation_authorized') 'package release'
    NonEmpty ([string]$Package.release.release_id) 'package release_id'
    NonEmpty ([string]$Package.release.release_version) 'package release_version'
    if ([string]$Package.release.status -notin @('draft','staged')) { Fail 'package release status is invalid' }
    if ($Package.release.production_eligible -ne $false -or $Package.release.activation_authorized -ne $false) { Fail 'package release cannot authorize production or activation' }
    Require-Array $Package.records 'package records'
    $records = @($Package.records)
    if ($records.Count -eq 0) { Fail 'package records must be non-empty' }
    $ids = @()
    for ($index = 0; $index -lt $records.Count; $index++) {
        Validate-Record $records[$index] $index
        if ($records[$index].record_id -in $ids) { Fail "duplicate record_id: $($records[$index].record_id)" }
        $ids += [string]$records[$index].record_id
    }
    return [ordered]@{
        schema_version = '0.1.0'
        package_id = [string]$Package.package_id
        package_kind = 'vietnamese-catalog-evidence'
        owner_decision_ref = $OwnerDecisionRef
        result = 'valid_candidate'
        candidate_only = $true
        production_eligible = $false
        activation_authorized = $false
        release_status = [string]$Package.release.status
        record_count = $records.Count
        record_kinds = @($records | ForEach-Object { [string]$_.record_kind } | Sort-Object -Unique)
        source_refs = @($Package.source_refs)
        records = @($records | ForEach-Object { [ordered]@{ record_id = [string]$_.record_id; record_kind = [string]$_.record_kind; review_status = [string]$_.review.status; release_status = [string]$_.release_status; production_eligible = $false } })
        decision_boundary = @('No catalog rows or release membership were written.', 'No production eligibility or activation was established.', 'Proposed review is not human approval; production evidence remains separately gated.')
    }
}
function Write-Report($Report) {
    if ([string]::IsNullOrWhiteSpace($OutputPath)) { $Report | ConvertTo-Json -Depth 30; return }
    $full = [IO.Path]::GetFullPath($OutputPath)
    $prefix = $RepositoryRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if ($full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { Fail 'OutputPath must be outside RepositoryRoot' }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $full) | Out-Null
    $Report | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $full -Encoding utf8
    Write-Output "[PASS] candidate-only catalog evidence report written: $full"
}
function Expect-Rejection($Package, [string]$Pattern, [string]$Context) {
    $failed = $false
    try { [void](Validate-Package $Package) }
    catch {
        $failed = $true
        if ($_.Exception.Message -notmatch $Pattern) { throw "[FAIL] $Context produced an unexpected error: $($_.Exception.Message)" }
    }
    if (-not $failed) { Fail "$Context unexpectedly passed" }
}
function New-SelfTestPackage {
    return [pscustomobject][ordered]@{
        schema_version = '0.1.0'; package_id = 'vmb-candidate-selftest'; package_kind = 'vietnamese-catalog-evidence'; owner_decision_ref = $OwnerDecisionRef
        source_refs = @([pscustomobject]@{source_ref='fixture://approved-source';sha256=('a' * 64)})
        release = [pscustomobject]@{release_id='catalog-candidate-selftest';release_version='0.1.0';status='draft';production_eligible=$false;activation_authorized=$false}
        records = @(
            [pscustomobject]@{
                record_id='identity-selftest';record_kind='identity';locale='vi-VN';canonical_food_id='food-selftest';canonical_name='Món kiểm thử'
                review=[pscustomobject]@{status='proposed';reviewer_ref='unassigned';reviewer_role='none';decision_ref=$OwnerDecisionRef}
                provenance=@([pscustomobject]@{evidence_ref='fixture://identity';sha256=('b' * 64)});release_status='draft';production_eligible=$false
            }
            [pscustomobject]@{
                record_id='alias-selftest';record_kind='alias';locale='vi-VN';canonical_food_id='food-selftest';alias='bí danh kiểm thử'
                review=[pscustomobject]@{status='proposed';reviewer_ref='unassigned';reviewer_role='none';decision_ref=$OwnerDecisionRef}
                provenance=@([pscustomobject]@{evidence_ref='fixture://alias';sha256=('c' * 64)});release_status='draft';production_eligible=$false
            }
            [pscustomobject]@{
                record_id='recipe-selftest';record_kind='recipe';locale='vi-VN';recipe_id='recipe-selftest'
                ingredients=@([pscustomobject]@{food_id='food-selftest';quantity=1;unit='fixture-unit'})
                cooked_yield=[pscustomobject]@{quantity=1;unit='fixture-serving'}
                review=[pscustomobject]@{status='proposed';reviewer_ref='unassigned';reviewer_role='none';decision_ref=$OwnerDecisionRef}
                provenance=@([pscustomobject]@{evidence_ref='fixture://recipe';sha256=('d' * 64)});release_status='draft';production_eligible=$false
            }
            [pscustomobject]@{
                record_id='portion-selftest';record_kind='portion';locale='vi-VN';food_id='food-selftest';preparation_state='fixture-preparation'
                measure=[pscustomobject]@{class='fixture-measure';code='fixture-code';context='fixture-context'};represented_quantity=1;study_id='study-selftest';protocol_version='protocol-selftest'
                independent_samples=@(10,11,12);estimate=[pscustomobject]@{lower_gram_weight=10;gram_weight=11;upper_gram_weight=12;sample_count=3}
                review=[pscustomobject]@{status='proposed';reviewer_ref='unassigned';reviewer_role='none';decision_ref=$OwnerDecisionRef}
                provenance=@([pscustomobject]@{evidence_ref='fixture://portion';sha256=('e' * 64)});release_status='draft';production_eligible=$false
            }
        )
    }
}
if ($SelfTest) {
    try {
        [void](Json $ContractPath)
        [void](Validate-Package (New-SelfTestPackage))
        $invalid = New-SelfTestPackage
        $invalid.release.production_eligible = $true
        Expect-Rejection $invalid 'production' 'invalid production eligibility self-test'
        $invalid = New-SelfTestPackage
        $invalid.records[0].review.reviewer_role = 'human_owner'
        Expect-Rejection $invalid 'proposed' 'invalid proposed human-review self-test'
        $invalid = New-SelfTestPackage
        $invalid.records = $invalid.records[0]
        Expect-Rejection $invalid 'must be an array' 'invalid records array-shape self-test'
        $invalid = New-SelfTestPackage
        $invalid.source_refs = $invalid.source_refs[0]
        Expect-Rejection $invalid 'must be an array' 'invalid source_refs array-shape self-test'
        $invalid = New-SelfTestPackage
        $invalid.records[2].provenance = $invalid.records[2].provenance[0]
        Expect-Rejection $invalid 'must be an array' 'invalid provenance array-shape self-test'
        $invalid = New-SelfTestPackage
        $invalid.records[2].ingredients = $invalid.records[2].ingredients[0]
        Expect-Rejection $invalid 'must be an array' 'invalid ingredients array-shape self-test'
        $invalid = New-SelfTestPackage
        $invalid.records[3].independent_samples = 11
        Expect-Rejection $invalid 'must be an array' 'invalid independent-samples array-shape self-test'
        $invalid = New-SelfTestPackage
        $invalid.source_refs[0].source_ref = 'C:\secure\source.json'
        Expect-Rejection $invalid 'unsafe' 'invalid Windows absolute reference self-test'
        $invalid = New-SelfTestPackage
        $invalid.source_refs[0].source_ref = 'Vietnam FCT 2017'
        Expect-Rejection $invalid 'prohibited' 'invalid prohibited-source self-test'
        $invalid = New-SelfTestPackage
        $invalid.records[0].record_id = 'Bad ID'
        Expect-Rejection $invalid 'pattern' 'invalid record identifier self-test'
        Write-Output '[PASS] Vietnamese catalog evidence validator self-test completed.'
        exit 0
    } catch { Write-Error $_.Exception.Message; exit 1 }
}
try {
    if ([string]::IsNullOrWhiteSpace($PackagePath)) { Fail '-PackagePath is required unless -SelfTest is used' }
    $package = Json ((Resolve-Path -LiteralPath $PackagePath -ErrorAction Stop).Path)
    Write-Report (Validate-Package $package)
    exit 0
} catch { Write-Error $_.Exception.Message; exit 1 }
