[CmdletBinding()]
param(
    [string]$TaskSpec,
    [string]$RepositoryRoot,
    [switch]$CiPolicy,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$script:ControlRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) { $script:RepoRoot = $script:ControlRoot }
else { $script:RepoRoot = (Resolve-Path -LiteralPath $RepositoryRoot -ErrorAction Stop).Path }

$script:ProtectedDomains = @(
    'product_domain_behavior', 'architectural_boundary', 'public_api_contract',
    'database_migration_intent', 'security_privacy_policy', 'llm_provider_boundary',
    'behavior_version_semantics', 'production_provider_infrastructure',
    'canonical_publication', 'release_policy', 'architecturally_significant_dependency_changes'
)
$script:RiskLevels = @('low', 'medium', 'high', 'critical')
$script:AuthorizationStates = @('unprotected', 'approved_protected_change', 'requires_human_decision')

function Fail([string]$Message) { throw "[FAIL] $Message" }
function Normalize-RepoPath([string]$Path) { if ($null -eq $Path) { return '' }; $value=$Path.Replace('\', '/'); if ($value.StartsWith('./')) { $value=$value.Substring(2) }; return $value }
function Get-RepoPath([string]$Root, [string]$RelativePath) { [IO.Path]::Combine($Root, ($RelativePath -replace '/', [IO.Path]::DirectorySeparatorChar)) }
function Load-Json([string]$Path) { if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail "JSON file does not exist: $Path" }; try { Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json } catch { Fail "invalid JSON: $Path" } }
function Has-Property($Object, [string]$Name) { $null -ne $Object.PSObject.Properties[$Name] }
function Require-Property($Object, [string]$Name, [string]$Context) { if (-not (Has-Property $Object $Name)) { Fail "$Context is missing '$Name'" }; return $Object.PSObject.Properties[$Name].Value }
function Assert-ExactProperties($Object, [string[]]$Allowed, [string]$Context) { $unknown = @($Object.PSObject.Properties.Name | Where-Object { $_ -notin $Allowed }); if ($unknown.Count -gt 0) { Fail "$Context contains unknown field(s): $($unknown -join ', ')" } }
function Assert-Array($Value, [string]$Context, [bool]$NonEmpty = $false) { if ($null -eq $Value -or $Value -is [string] -or $Value -isnot [System.Collections.IEnumerable]) { Fail "$Context must be an array" }; $items = @($Value); if ($NonEmpty -and $items.Count -eq 0) { Fail "$Context must be non-empty" }; return $items }
function Assert-StringArray($Value, [string]$Context, [bool]$NonEmpty = $false) { $items = @(Assert-Array $Value $Context $NonEmpty); if (@($items | Where-Object { $_ -isnot [string] -or [string]::IsNullOrWhiteSpace($_) }).Count -gt 0) { Fail "$Context must contain non-empty strings" }; return $items }
function Assert-NonNegativeInteger($Value, [string]$Context) { if ($Value -isnot [int] -and $Value -isnot [long] -and $Value -isnot [double]) { Fail "$Context must be numeric" }; if ([double]$Value -lt 0 -or [double]$Value -ne [math]::Floor([double]$Value)) { Fail "$Context must be a non-negative integer" } }
function Test-GlobMatch([string]$Path, [string]$Pattern) { $escaped = [regex]::Escape((Normalize-RepoPath $Pattern)).Replace('\*\*', '.*').Replace('\*', '[^/]*').Replace('\?', '[^/]'); return [regex]::IsMatch((Normalize-RepoPath $Path), "^$escaped$", [Text.RegularExpressions.RegexOptions]::IgnoreCase) }
function Get-RelativeFiles([string]$Root, [string]$RelativeDirectory) { $directory = Get-RepoPath $Root $RelativeDirectory; if (-not (Test-Path -LiteralPath $directory -PathType Container)) { return @() }; return @(Get-ChildItem -LiteralPath $directory -Recurse -File | ForEach-Object { Normalize-RepoPath $_.FullName.Substring($Root.Length + 1) }) }

function Get-CanonicalGates([string]$Root) { $map = Load-Json (Get-RepoPath $Root '.agent/maps/verification-map.json'); return @($map.gates | ForEach-Object { [string]$_.name }) }
function Get-ContextModules([string]$Root) { $modules = Load-Json (Get-RepoPath $Root '.agent/context/modules.json'); return @($modules.modules | ForEach-Object { [string]$_.name }) }

function Assert-RequiredFiles([string]$Root) {
    $required = @(
        'AGENTS.md', 'docs/AGENT_ENGINEERING.md', '.agent/README.md', '.agent/manifest.json',
        '.agent/authority/execution-contract.md', '.agent/authority/decision-policy.md', '.agent/authority/escalation-policy.md',
        '.agent/invariants/product-domain.md', '.agent/invariants/architecture.md', '.agent/invariants/data-replay.md',
        '.agent/invariants/llm-boundary.md', '.agent/invariants/security-privacy.md',
        '.agent/contexts/domain.md', '.agent/contexts/application.md', '.agent/contexts/parser.md', '.agent/contexts/persistence.md',
        '.agent/contexts/api.md', '.agent/contexts/worker.md', '.agent/contexts/data-governance.md', '.agent/contexts/verification.md',
        '.agent/context/modules.json', '.agent/context/router.json',
        '.agent/maps/verification-map.json', '.agent/maps/source-register.json', '.agent/verification/risk-policy.json', '.agent/verification/scope-policy.json',
        '.agent/contracts/task-intent.schema.json', '.agent/contracts/task-spec.schema.json',
        '.agent/contracts/verification-report.schema.json', '.agent/contracts/implementation-report.schema.json', '.agent/contracts/external-evidence.schema.json', '.agent/contracts/ci-attestation.schema.json',
        '.agent/evals/README.md',
        'scripts/compile-agent-task-spec.ps1', 'scripts/prepare-agent-task.ps1', 'scripts/verify-agent-context.ps1', 'scripts/run-agent-verification.ps1', 'scripts/verify-agent-behavior.ps1',
        '.github/workflows/agent-context-integrity.yml', '.github/workflows/agent-task-attest.yml'
    )
    foreach ($path in $required) { if (-not (Test-Path -LiteralPath (Get-RepoPath $Root $path) -PathType Leaf)) { Fail "required active file is missing: $path" } }
}

function Assert-Manifest([string]$Root) {
    $manifest = Load-Json (Get-RepoPath $Root '.agent/manifest.json')
    Assert-ExactProperties $manifest @('schema_version','verifier_release','verification_registry_release','runner_release','ci_release','ci_attestation_contract_release','project','authority','budgets','paths') 'manifest'
    if ([string]$manifest.schema_version -ne '1.0.0') { Fail 'manifest schema_version mismatch' }
    $expected = @{
        verifier_release='agent-verifier-3.0.0'; verification_registry_release='agent-gates-3.0.0'; runner_release='agent-runner-2.0.0'; ci_release='agent-ci-2.0.0'; ci_attestation_contract_release='agent-ci-attestation-2.0.0'
    }
    foreach ($key in $expected.Keys) { if ([string]$manifest.$key -ne $expected[$key]) { Fail "manifest $key release mismatch" } }
    if ([string]$manifest.project.repository -ne 'pumni/Nutrition_backend') { Fail 'manifest repository mismatch' }
    $authority = $manifest.authority
    Assert-ExactProperties $authority @('architect_decides','implementation_autonomous_within_policy','protected_decisions_fail_closed','task_spec_required','context_routing_required') 'manifest authority'
    if ($authority.architect_decides -ne $true -or $authority.implementation_autonomous_within_policy -ne $true -or $authority.protected_decisions_fail_closed -ne $true -or $authority.task_spec_required -ne $true -or $authority.context_routing_required -ne $true) { Fail 'manifest authority model is invalid' }
    Assert-ExactProperties $manifest.paths @('context_router','context_modules','source_register','verification_map') 'manifest paths'
    return $manifest
}

function Assert-Budgets([string]$Root, $Manifest) {
    if ((Get-Item -LiteralPath (Get-RepoPath $Root 'AGENTS.md')).Length -gt [int]$Manifest.budgets.agents_md_max_bytes) { Fail 'AGENTS.md exceeds budget' }
    if ((Get-Item -LiteralPath (Get-RepoPath $Root '.agent/README.md')).Length -gt 8192) { Fail '.agent/README.md exceeds budget' }
    foreach ($pair in @(@('.agent/authority', $Manifest.budgets.authority_file_max_bytes), @('.agent/invariants', $Manifest.budgets.invariant_file_max_bytes), @('.agent/contexts', $Manifest.budgets.context_file_max_bytes))) { foreach ($file in Get-ChildItem -LiteralPath (Get-RepoPath $Root $pair[0]) -File) { if ($file.Length -gt [int64]$pair[1]) { Fail "$($file.FullName) exceeds context budget" } } }
    $modules=Load-Json (Get-RepoPath $Root '.agent/context/modules.json'); $router=Load-Json (Get-RepoPath $Root '.agent/context/router.json'); $default=@($router.default_modules); $initial=@(Get-RepoPath $Root 'AGENTS.md'); foreach ($name in $default) { $module=@($modules.modules | Where-Object name -eq $name)[0]; $initial += @($module.context_files | ForEach-Object { Get-RepoPath $Root $_ }) }; $initialBytes=(@($initial | Select-Object -Unique | ForEach-Object { (Get-Item -LiteralPath $_).Length } | Measure-Object -Sum).Sum); if ([int64]$initialBytes -gt [int64]$Manifest.budgets.initial_guidance_max_bytes) { Fail "initial routed guidance exceeds $($Manifest.budgets.initial_guidance_max_bytes) bytes: $initialBytes" }
}

function Assert-Entrypoint([string]$Root) {
    $text = Get-Content -Raw (Get-RepoPath $Root 'AGENTS.md')
    foreach ($required in @('.agent/manifest.json','docs/AGENT_ENGINEERING.md','Task Intent','minimal modules','scope envelope','canonical gate IDs','protected-decision report')) { if ($text -notmatch [regex]::Escape($required)) { Fail "AGENTS.md is missing guidance: $required" } }
    foreach ($forbidden in @('Transitional executor model','obsolete profile selection')) { if ($text -match [regex]::Escape($forbidden)) { Fail "AGENTS.md retains stale guidance: $forbidden" } }
}

function Assert-SourceRegister([string]$Root) {
    $register = Load-Json (Get-RepoPath $Root '.agent/maps/source-register.json')
    foreach ($entry in $register.PSObject.Properties) {
        if (-not (Test-Path -LiteralPath (Get-RepoPath $Root $entry.Name) -PathType Leaf)) { Fail "source register artifact is missing: $($entry.Name)" }
        foreach ($source in @($entry.Value)) { $path = Normalize-RepoPath ([string]$source); if ($path -ne [string]$source) { Fail "source register path is not normalized: $source" }; if (-not (Test-Path -LiteralPath (Get-RepoPath $Root $path) -PathType Leaf)) { Fail "source register source is missing: $path" } }
    }
}
function Assert-VerificationMap([string]$Root) {
    $map = Load-Json (Get-RepoPath $Root '.agent/maps/verification-map.json'); Assert-ExactProperties $map @('schema_version','release','gates') 'verification map'; if ($map.schema_version -ne '3.0.0' -or $map.release -ne 'agent-gates-3.0.0') { Fail 'verification map release mismatch' }
    $names=@(); foreach ($gate in @($map.gates)) { Assert-ExactProperties $gate @('name','kind','script','arguments','program','paths','evidence_kind','display_command','target_root_argument') 'verification gate'; if ([string]::IsNullOrWhiteSpace([string]$gate.name)) { Fail 'verification gate ID is empty' }; if ($gate.name -in $names) { Fail "duplicate verification gate: $($gate.name)" }; $names += [string]$gate.name; if ([string]$gate.kind -notin @('control-script','target-script','native','json-parse','external-evidence')) { Fail "unknown verification gate kind: $($gate.name)" } }
    return $names
}

function Assert-ContextRouting([string]$Root, [string[]]$KnownGates) {
    $modulesDocument = Load-Json (Get-RepoPath $Root '.agent/context/modules.json'); Assert-ExactProperties $modulesDocument @('schema_version','modules') 'context modules'; if ($modulesDocument.schema_version -ne '3.0.0') { Fail 'context modules schema mismatch' }
    $modules=@(); foreach ($module in @($modulesDocument.modules)) { Assert-ExactProperties $module @('name','context_files','risk_tags') 'context module'; if ($module.name -in $modules) { Fail "duplicate context module: $($module.name)" }; $modules += [string]$module.name; foreach ($path in @(Assert-StringArray $module.context_files "context files $($module.name)" $true)) { if (-not (Test-Path -LiteralPath (Get-RepoPath $Root $path) -PathType Leaf)) { Fail "context module path missing: $path" } } }
    $router = Load-Json (Get-RepoPath $Root '.agent/context/router.json'); Assert-ExactProperties $router @('schema_version','modules_ref','default_modules','path_routes','expansion_policy') 'context router'; if ($router.schema_version -ne '3.0.0' -or $router.modules_ref -ne '.agent/context/modules.json') { Fail 'context router identity mismatch' }
    foreach ($module in @($router.default_modules)) { if ($module -notin $modules) { Fail "router default module is unknown: $module" } }
    foreach ($route in @($router.path_routes)) { foreach ($module in @($route.modules)) { if ($module -notin $modules) { Fail "router path route module is unknown: $module" } }; foreach ($gate in @($route.mandatory_gates)) { if ($gate -notin $KnownGates) { Fail "router path route gate is unknown: $gate" } } }
    return $modules
}

function Assert-RiskPolicy([string]$Root) {
    $policy = Load-Json (Get-RepoPath $Root '.agent/verification/risk-policy.json'); Assert-ExactProperties $policy @('schema_version','risk_levels','protected_domains') 'risk policy'; if ($policy.schema_version -ne '2.0.0') { Fail 'risk policy schema mismatch' }
    foreach ($level in @($policy.risk_levels)) { if ($level -notin $script:RiskLevels) { Fail "unknown risk level: $level" } }
    foreach ($property in $policy.protected_domains.PSObject.Properties) { if ($property.Name -notin $script:ProtectedDomains) { Fail "risk policy has unknown protected domain: $($property.Name)" }; if ($property.Value -notin $script:RiskLevels) { Fail "risk policy domain risk is invalid: $($property.Name)" } }
    foreach ($domain in $script:ProtectedDomains) { if (-not (Has-Property $policy.protected_domains $domain)) { Fail "risk policy is missing protected domain: $domain" } }
}
function Assert-ScopePolicy([string]$Root) {
    $policy=Load-Json (Get-RepoPath $Root '.agent/verification/scope-policy.json'); Assert-ExactProperties $policy @('schema_version','protected_path_patterns','approval_source','protected_scope_requires_approval') 'scope policy'; if ($policy.schema_version -ne '2.0.0' -or $policy.approval_source -ne 'task_spec.scope_envelope.approved_protected_paths' -or $policy.protected_scope_requires_approval -ne $true) { Fail 'scope policy is invalid' }; [void](Assert-StringArray $policy.protected_path_patterns 'scope protected path patterns' $true)
}

function Assert-TaskSpecV2([string]$Root, $Spec) {
    $allowed=@('schema_version','task_id','objective','acceptance_criteria','non_negotiables','risk_level','scope_envelope','approved_protected_decisions','baseline'); Assert-ExactProperties $Spec $allowed 'Task Spec'; if ($Spec.schema_version -ne '2.0.0') { Fail 'Task Spec schema_version must be 2.0.0' }; if ([string]::IsNullOrWhiteSpace([string]$Spec.task_id) -or [string]::IsNullOrWhiteSpace([string]$Spec.objective)) { Fail 'Task Spec identity/objective is empty' }; [void](Assert-StringArray $Spec.acceptance_criteria 'Task Spec acceptance_criteria' $true); [void](Assert-StringArray $Spec.non_negotiables 'Task Spec non_negotiables'); if ($Spec.risk_level -notin $script:RiskLevels) { Fail 'Task Spec risk_level is invalid' }
    Assert-ExactProperties $Spec.scope_envelope @('include','exclude','approved_protected_paths') 'Task Spec scope_envelope'; [void](Assert-StringArray $Spec.scope_envelope.include 'Task Spec scope include' $true); [void](Assert-StringArray $Spec.scope_envelope.exclude 'Task Spec scope exclude'); [void](Assert-StringArray $Spec.scope_envelope.approved_protected_paths 'Task Spec approved protected paths')
    foreach ($approval in @(Assert-Array $Spec.approved_protected_decisions 'Task Spec approvals')) { Assert-ExactProperties $approval @('domain','decision_id','approval_ref','scope') 'Task Spec protected approval'; if ($approval.domain -notin $script:ProtectedDomains) { Fail "Task Spec approval domain is invalid: $($approval.domain)" }; foreach ($field in @('decision_id','approval_ref')) { if ([string]::IsNullOrWhiteSpace([string]$approval.$field)) { Fail "Task Spec approval $field is empty" } }; [void](Assert-StringArray $approval.scope 'Task Spec approval scope' $true) }
    Assert-ExactProperties $Spec.baseline @('commit','source') 'Task Spec baseline'; if ([string]$Spec.baseline.commit -notmatch '^[0-9a-fA-F]{40}$' -or $Spec.baseline.source -ne 'git') { Fail 'Task Spec baseline is invalid' }
}

function Assert-Contracts([string]$Root, [string[]]$KnownGates) {
    $taskSchema=Load-Json (Get-RepoPath $Root '.agent/contracts/task-spec.schema.json'); if ($taskSchema.properties.schema_version.const -ne '2.0.0') { Fail 'Task Spec contract release mismatch' }
    $reportSchema=Load-Json (Get-RepoPath $Root '.agent/contracts/implementation-report.schema.json'); if ($reportSchema.properties.schema_version.const -ne '2.0.0') { Fail 'implementation report schema release mismatch' }; $verification=$reportSchema.properties.verification.items; if (@($verification.required) -join ',' -ne 'gate_id,status,evidence_ref') { Fail 'implementation report verification shape is not strict v2' }; if ($reportSchema | ConvertTo-Json -Depth 20 | Select-String -Pattern 'command|oneOf') { Fail 'implementation report contract contains command truth or dual mode' }
    $schema=Load-Json (Get-RepoPath $Root '.agent/contracts/verification-report.schema.json'); if ($schema.properties.schema_version.const -ne '2.0.0') { Fail 'verification report contract release mismatch' }
    $intentPath = Get-RepoPath $Root '.agent/templates/task-intent.example.json'
    if (Test-Path -LiteralPath $intentPath -PathType Leaf) { $intent=Load-Json $intentPath; Assert-ExactProperties $intent @('task_id','objective','acceptance_criteria','non_negotiables','scope_hints','scope_exclusions','risk_floor','approved_protected_decisions') 'Task intent'; [void](Assert-StringArray $intent.acceptance_criteria 'Task intent acceptance_criteria' $true); if ($intent.risk_floor -and $intent.risk_floor -notin $script:RiskLevels) { Fail 'Task intent risk_floor is invalid' } }
    $evidenceExample = Get-RepoPath $Root '.agent/templates/external-evidence.example.json'
    if (Test-Path -LiteralPath $evidenceExample -PathType Leaf) { $evidence=Load-Json $evidenceExample; if ($evidence.schema_version -ne '1.0.0') { Fail 'external evidence example schema mismatch' } }
}

function Assert-ContextText([string]$Root) {
    $active = @('AGENTS.md','README.md','docs/AGENT_ENGINEERING.md','.agent/README.md') + @(Get-ChildItem -LiteralPath (Get-RepoPath $Root '.agent') -Recurse -File | ForEach-Object { Normalize-RepoPath $_.FullName.Substring($Root.Length + 1) })
    $archivedPrefix = 'docs/' + 'archive/'
    foreach ($path in $active) { $text=Get-Content -Raw (Get-RepoPath $Root $path); if ($text -match '(?i)transitional\s+v1|compatibility\s+architecture') { Fail "active context contains retired migration guidance: $path" }; if ($text -match [regex]::Escape($archivedPrefix)) { Fail "active context points at archived normative material: $path" } }
}

function Assert-CiPolicy([string]$Root) {
    $ciPolicy=Load-Json (Get-RepoPath $Root '.agent/maps/ci-policy.json'); if ($ciPolicy.schema_version -ne '2.0.0' -or $ciPolicy.release -ne 'agent-ci-2.0.0') { Fail 'CI policy release mismatch' }
    $attestation=Load-Json (Get-RepoPath $Root '.agent/contracts/ci-attestation.schema.json'); if ($attestation.properties.schema_version.const -ne '2.0.0' -or $attestation.properties.release.const -ne 'agent-ci-attestation-2.0.0' -or $attestation.properties.runner_release.const -ne 'agent-runner-2.0.0' -or $attestation.properties.verifier_release.const -ne 'agent-verifier-3.0.0' -or $attestation.required -notcontains 'baseline_sha') { Fail 'CI attestation contract release or baseline binding is invalid' }
    $integrity=Get-RepoPath $Root '.github/workflows/agent-context-integrity.yml'; $attest=Get-RepoPath $Root '.github/workflows/agent-task-attest.yml'; foreach ($path in @($integrity,$attest)) { if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { Fail "CI workflow missing: $path" } }
    $text=Get-Content -Raw $integrity; foreach ($required in @('verify-agent-context.ps1 -SelfTest','run-agent-verification.ps1 -SelfTest','verify-agent-context.ps1 -CiPolicy')) { if ($text -notmatch [regex]::Escape($required)) { Fail "integrity workflow missing check: $required" } }; if ($text -match '(?m)contents:\s*write') { Fail 'integrity workflow grants contents write' }
    $attestText=Get-Content -Raw $attest; if ($attestText -notmatch 'workflow_dispatch' -or $attestText -notmatch 'baseline_sha' -or $attestText -notmatch 'prepare-agent-task.ps1' -or $attestText -notmatch '-TaskSpec') { Fail 'attestation workflow is missing explicit prepare/verify baseline binding' }; if ($attestText -match '(?m)pull_request_target|workflow_run') { Fail 'attestation workflow has an unsafe trigger' }
    foreach ($workflow in @($text,$attestText)) { foreach ($match in [regex]::Matches($workflow, 'uses:\s*[^\s]+@([^\s#]+)')) { if ($match.Groups[1].Value -notmatch '^[0-9a-fA-F]{40}$') { Fail 'CI action reference is not pinned to a full commit' } } }
}

function Invoke-Git([string]$Root, [string[]]$Arguments) { $output=& git -C $Root @Arguments 2>&1 | Out-String; if ($LASTEXITCODE -ne 0) { Fail "git failed: $($Arguments -join ' ')" }; return $output.Trim() }
function Get-ActualChangeRecords([string]$Root, [string]$Baseline) {
    $records=@(); $status=(Invoke-Git $Root @('diff','--name-status','--no-renames',$Baseline,'--')) -split "`r?`n" | Where-Object { $_ }; foreach ($line in $status) { $parts=$line -split "`t"; $type=if($parts[0] -match 'A'){ 'create' } elseif($parts[0] -match 'D'){ 'delete' } else { 'modify' }; $records += [pscustomobject]@{path=(Normalize-RepoPath $parts[-1]);type=$type} }; $untracked=(Invoke-Git $Root @('ls-files','--others','--exclude-standard')) -split "`r?`n" | Where-Object { $_ }; foreach ($path in $untracked) { $records += [pscustomobject]@{path=(Normalize-RepoPath $path);type='create'} }; return @($records | Sort-Object path -Unique)
}
function Assert-Scope([string]$Root, $Spec) {
    $records=Get-ActualChangeRecords $Root ([string]$Spec.baseline.commit); $policy=Load-Json (Get-RepoPath $Root '.agent/verification/scope-policy.json'); $approved=@($Spec.scope_envelope.approved_protected_paths) + @($Spec.approved_protected_decisions | ForEach-Object { $_.scope })
    foreach ($record in $records) {
        $inside=@($Spec.scope_envelope.include | Where-Object { Test-GlobMatch $record.path $_ }).Count -gt 0; $excluded=@($Spec.scope_envelope.exclude | Where-Object { Test-GlobMatch $record.path $_ }).Count -gt 0; if (-not $inside -or $excluded) { Fail "SCOPE_VIOLATION: $($record.path)" }
        $protected=@($policy.protected_path_patterns | Where-Object { Test-GlobMatch $record.path $_ }).Count -gt 0; if ($protected -and @($approved | Where-Object { Test-GlobMatch $record.path $_ }).Count -eq 0) { Fail "PROTECTED_DECISION_REQUIRED: $($record.path)" }
    }
    return $records
}

function Assert-Integrity([string]$Root) {
    Assert-RequiredFiles $Root; $manifest=Assert-Manifest $Root; Assert-Budgets $Root $manifest; Assert-Entrypoint $Root; Assert-ContextText $Root; Assert-SourceRegister $Root; $knownGates=@(Assert-VerificationMap $Root); [void](Assert-ContextRouting $Root $knownGates); Assert-RiskPolicy $Root; Assert-ScopePolicy $Root; Assert-Contracts $Root $knownGates; Assert-CiPolicy $Root; Write-Output '[PASS] Active agent context integrity verified.'
}

function Assert-SelfTestExpected([string]$Name, [bool]$ExpectedPass, [scriptblock]$Action) { $passed=$false; try { & $Action; $passed=$true } catch { $passed=$false }; if ($passed -ne $ExpectedPass) { Fail "self-test case failed: $Name" }; Write-Output "[PASS] Self-test: $Name" }
function Invoke-SourceRegisterSelfTests([string]$Root) {
    $fixture=Join-Path ([IO.Path]::GetTempPath()) ('agent-context-source-register-' + [guid]::NewGuid().ToString('N')); New-Item -ItemType Directory -Force -Path $fixture | Out-Null
    try {
        Copy-Item -Recurse -Force (Get-RepoPath $Root '.agent') (Join-Path $fixture '.agent')
        $register=Load-Json (Get-RepoPath $Root '.agent/maps/source-register.json')
        foreach ($entry in $register.PSObject.Properties) {
            foreach ($source in @($entry.Value)) {
                $dest=Get-RepoPath $fixture ([string]$source); New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dest) | Out-Null; Copy-Item -Force (Get-RepoPath $Root ([string]$source)) $dest
            }
        }
        Assert-SourceRegister $fixture; Write-Output '[PASS] Source register baseline'
        $entry=$register.PSObject.Properties | Select-Object -First 1
        $missing=[string]$entry.Value[0]; Remove-Item -LiteralPath (Get-RepoPath $fixture $missing) -Force; Assert-SelfTestExpected 'missing authoritative source fails closed' $false { Assert-SourceRegister $fixture }
    } finally { if (Test-Path -LiteralPath $fixture) { Remove-Item -Recurse -Force $fixture } }
}
function Invoke-SelfTest([string]$Root) { Invoke-SourceRegisterSelfTests $Root; Assert-Integrity $Root; Write-Output '[PASS] Modern agent context self-test completed.' }

Push-Location $script:RepoRoot
try {
    if ($SelfTest -and ($CiPolicy -or $TaskSpec)) { Fail '-SelfTest cannot be combined with other execution modes' }
    if ($CiPolicy -and ($SelfTest -or $TaskSpec)) { Fail '-CiPolicy cannot be combined with other execution modes' }
    if ($SelfTest) { Invoke-SelfTest $script:RepoRoot; exit 0 }
    if ($CiPolicy) { Assert-CiPolicy $script:RepoRoot; Write-Output '[PASS] CI policy verification passed.'; exit 0 }
    Assert-Integrity $script:RepoRoot
    if ($TaskSpec) { $path=if([IO.Path]::IsPathRooted($TaskSpec)){$TaskSpec}else{Get-RepoPath $script:RepoRoot $TaskSpec}; $spec=Load-Json $path; Assert-TaskSpecV2 $script:RepoRoot $spec; [void](Assert-Scope $script:RepoRoot $spec); Write-Output "[PASS] Task Spec validated: $($spec.task_id)" }
}
catch { Write-Error $_.Exception.Message; exit 1 }
finally { Pop-Location }
