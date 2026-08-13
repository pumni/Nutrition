[CmdletBinding()]
param(
    [string]$TaskPacket,
    [string]$RepositoryRoot,
    [string]$TaskStateOutput,
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$script:ControlRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $script:RepoRoot = $script:ControlRoot
}
else {
    try { $script:RepoRoot = (Resolve-Path -LiteralPath $RepositoryRoot -ErrorAction Stop).Path }
    catch { throw "[FAIL] RepositoryRoot does not exist: $RepositoryRoot" }
}

function Fail([string]$Message) {
    throw "[FAIL] $Message"
}

function Normalize-RepoPath([string]$Path) {
    if ($null -eq $Path) { return "" }
    $normalized = $Path.Replace("\", "/")
    if ($normalized.StartsWith("./")) { $normalized = $normalized.Substring(2) }
    return $normalized
}

function Get-RepoPath([string]$Root, [string]$RelativePath) {
    return (Join-Path $Root ($RelativePath.Replace("/", "\")))
}

function Get-FullPath([string]$Path) {
    return [IO.Path]::GetFullPath($Path)
}

function Assert-PathOutsideRoot([string]$Path, [string]$Root, [string]$Label) {
    if ([string]::IsNullOrWhiteSpace($Path)) { Fail "$Label path is empty" }
    $fullPath = Get-FullPath $Path
    $fullRoot = (Get-FullPath $Root).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    if ($fullPath.Equals($fullRoot.TrimEnd([IO.Path]::DirectorySeparatorChar), [StringComparison]::OrdinalIgnoreCase) -or $fullPath.StartsWith($fullRoot, [StringComparison]::OrdinalIgnoreCase)) {
        Fail "$Label must be outside RepositoryRoot: $fullPath"
    }
    return $fullPath
}

function Has-Property($Object, [string]$Name) {
    return $null -ne $Object.PSObject.Properties[$Name]
}

function Require-Property($Object, [string]$Name, [string]$Context) {
    if (-not (Has-Property $Object $Name)) { Fail "$Context is missing required field '$Name'" }
    return $Object.PSObject.Properties[$Name].Value
}

function Assert-ExactProperties($Object, [string[]]$Allowed, [string]$Context) {
    $actual = @($Object.PSObject.Properties.Name)
    $unknown = @($actual | Where-Object { $_ -notin $Allowed })
    if ($unknown.Count -gt 0) { Fail "$Context contains unknown field(s): $($unknown -join ', ')" }
}

function Assert-Array($Value, [string]$Context, [bool]$NonEmpty = $false) {
    if ($null -eq $Value -or $Value -is [string] -or $Value -isnot [System.Collections.IEnumerable]) { Fail "$Context must be an array" }
    $items = @($Value)
    if ($NonEmpty -and $items.Count -eq 0) { Fail "$Context must be non-empty" }
    return $items
}

function Assert-StringArray($Value, [string]$Context, [bool]$NonEmpty = $false) {
    $items = @(Assert-Array $Value $Context $NonEmpty)
    if (@($items | Where-Object { $_ -isnot [string] -or [string]::IsNullOrWhiteSpace($_) }).Count -gt 0) { Fail "$Context must contain non-empty strings" }
    return $items
}

function Load-Json([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail "JSON file does not exist: $Path" }
    try {
        return Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
    }
    catch {
        Fail "Invalid JSON: $Path :: $($_.Exception.Message)"
    }
}

function Test-GlobMatch([string]$Path, [string]$Pattern) {
    $pathValue = Normalize-RepoPath $Path
    $patternValue = Normalize-RepoPath $Pattern
    $escaped = [regex]::Escape($patternValue)
    $escaped = $escaped.Replace("\*\*", ".*").Replace("\*", "[^/]*").Replace("\?", "[^/]")
    return [regex]::IsMatch($pathValue, "^$escaped$")
}

function Get-RelativeFiles([string]$Root, [string]$RelativeDirectory) {
    $directory = Get-RepoPath $Root $RelativeDirectory
    if (-not (Test-Path -LiteralPath $directory -PathType Container)) { return @() }
    return @(Get-ChildItem -LiteralPath $directory -Recurse -File | ForEach-Object {
        Normalize-RepoPath $_.FullName.Substring($Root.Length + 1)
    })
}

function Assert-RequiredAclFiles([string]$Root) {
    $required = @(
        ".agent/README.md",
        ".agent/manifest.json",
        ".agent/authority/executor-contract.md",
        ".agent/authority/decision-policy.md",
        ".agent/authority/escalation-protocol.md",
        ".agent/invariants/product-domain.md",
        ".agent/invariants/architecture.md",
        ".agent/invariants/data-replay.md",
        ".agent/invariants/llm-boundary.md",
        ".agent/invariants/security-privacy.md",
        ".agent/contexts/foundation.md",
        ".agent/contexts/domain.md",
        ".agent/contexts/application.md",
        ".agent/contexts/parser.md",
        ".agent/contexts/persistence.md",
        ".agent/contexts/api.md",
        ".agent/contexts/worker.md",
        ".agent/contexts/data-governance.md",
        ".agent/contexts/verification.md",
        ".agent/maps/crate-map.json",
        ".agent/maps/change-impact-map.json",
        ".agent/maps/verification-map.json",
        ".agent/maps/source-register.json",
        ".agent/profiles/context-profiles.json",
        ".agent/contracts/task-packet.schema.json",
        ".agent/contracts/verification-report.schema.json",
        ".agent/contracts/implementation-report.schema.json",
        ".agent/templates/task-packet.example.json",
        ".agent/templates/verification-report.example.json",
        ".agent/templates/implementation-report.example.md",
        ".agent/evals/README.md",
        ".agent/evals/context-layer-cases.json",
        ".agent/evals/runner-cases.json",
        ".agent/contracts/external-evidence.schema.json",
        ".agent/state/source-lock.json",
        "scripts/verify-agent-context.ps1",
        "scripts/run-agent-verification.ps1",
        ".agent/templates/external-evidence.example.json"
    )
    foreach ($path in $required) {
        if (-not (Test-Path -LiteralPath (Get-RepoPath $Root $path) -PathType Leaf)) {
            Fail "required ACL file is missing: $path"
        }
    }
}

function Assert-JsonArtifacts([string]$Root) {
    foreach ($path in (Get-RelativeFiles $Root ".agent" | Where-Object { $_.EndsWith(".json") })) {
        [void](Load-Json (Get-RepoPath $Root $path))
    }
}

function Assert-Manifest([string]$Root) {
    $manifest = Load-Json (Get-RepoPath $Root ".agent/manifest.json")
    Assert-ExactProperties $manifest @("schema_version", "context_release", "contract_release", "verifier_release", "verification_registry_release", "runner_release", "verification_report_release", "implementation_report_release", "project", "authority", "budgets", "paths") "manifest"
    if ((Require-Property $manifest "schema_version" "manifest") -ne "1.0.0") { Fail "manifest schema_version must be 1.0.0" }
    if ((Require-Property $manifest "context_release" "manifest") -ne "agent-context-1.0.0") { Fail "manifest context_release must be agent-context-1.0.0" }
    if ((Require-Property $manifest "contract_release" "manifest") -ne "agent-contract-1.1.0") { Fail "manifest contract_release mismatch" }
    if ((Require-Property $manifest "verifier_release" "manifest") -ne "agent-verifier-2.1.0") { Fail "manifest verifier_release mismatch" }
    if ((Require-Property $manifest "verification_registry_release" "manifest") -ne "agent-gates-2.1.0") { Fail "manifest verification_registry_release mismatch" }
    if ((Require-Property $manifest "runner_release" "manifest") -ne "agent-runner-1.0.0") { Fail "manifest runner_release mismatch" }
    if ((Require-Property $manifest "verification_report_release" "manifest") -ne "agent-verification-report-2.0.0") { Fail "manifest verification_report_release mismatch" }
    if ((Require-Property $manifest "implementation_report_release" "manifest") -ne "agent-implementation-report-1.1.0") { Fail "manifest implementation_report_release mismatch" }
    if ((Require-Property $manifest "project" "manifest").repository -ne "pumni/Nutrition_backend") { Fail "manifest repository mismatch" }
    if ((Require-Property $manifest "project" "manifest").behavior_release -ne "foundation-0.6.0") { Fail "manifest behavior_release mismatch" }
    $paths = Require-Property $manifest "paths" "manifest"
    foreach ($name in @("profile_index", "source_register", "source_lock", "verification_map")) {
        [void](Require-Property $paths $name "manifest.paths")
    }
    return $manifest
}

function Assert-Budgets([string]$Root, $Manifest) {
    $agents = Get-RepoPath $Root "AGENTS.md"
    if (Test-Path -LiteralPath $agents -PathType Leaf) {
        if ((Get-Item -LiteralPath $agents).Length -gt 4096) { Fail "AGENTS.md exceeds 4096 bytes" }
        if ((Get-Content -Raw -LiteralPath $agents) -notmatch "\.agent/manifest\.json") { Fail "AGENTS.md does not point to .agent/manifest.json" }
    }
    else {
        Write-Warning "[WARN] AGENTS.md is absent; root-entrypoint check is deferred until P06."
    }
    $budget = $Manifest.budgets
    $readmeLimit = 8192
    if ((Get-Item -LiteralPath (Get-RepoPath $Root ".agent/README.md")).Length -gt $readmeLimit) { Fail ".agent/README.md exceeds 8192 bytes" }
    foreach ($directory in @(@(".agent/authority", $budget.authority_file_max_bytes), @(".agent/invariants", $budget.invariant_file_max_bytes), @(".agent/contexts", $budget.context_file_max_bytes))) {
        foreach ($file in (Get-ChildItem -LiteralPath (Get-RepoPath $Root $directory[0]) -File)) {
            if ($file.Length -gt [int64]$directory[1]) { Fail "$($file.FullName) exceeds its context budget" }
        }
    }
}

function Assert-Profiles([string]$Root) {
    $profilesDocument = Load-Json (Get-RepoPath $Root ".agent/profiles/context-profiles.json")
    if ((Require-Property $profilesDocument "schema_version" "context profiles") -ne "1.0.0") { Fail "context profile schema_version mismatch" }
    $profiles = @((Require-Property $profilesDocument "profiles" "context profiles"))
    if ($profiles.Count -eq 0) { Fail "context profile list is empty" }
    $duplicateNames = @($profiles | Group-Object -Property name | Where-Object Count -gt 1)
    if ($duplicateNames.Count -gt 0) { Fail "context profile names are not unique" }
    foreach ($profile in $profiles) {
        $name = Require-Property $profile "name" "profile"
        $required = @((Require-Property $profile "required_context_files" "profile $name"))
        $gates = @((Require-Property $profile "mandatory_verification_gates" "profile $name"))
        if ($required.Count -eq 0) { Fail "profile $name has no required context files" }
        if ($gates.Count -eq 0) { Fail "profile $name has an empty verification gate list" }
        foreach ($path in $required) {
            if (-not (Test-Path -LiteralPath (Get-RepoPath $Root $path) -PathType Leaf)) { Fail "profile $name references missing file: $path" }
        }
    }
    $maintenance = $profiles | Where-Object name -eq "agent-context-maintenance"
    if ($null -eq $maintenance) { Fail "agent-context-maintenance profile is missing" }
    $forbidden = @($maintenance.forbidden_path_patterns)
    if ($forbidden -notcontains "crates/**" -or $forbidden -notcontains "migrations/**") { Fail "agent-context-maintenance must forbid crates/** and migrations/**" }
    return $profiles
}

function Assert-SourceRegister([string]$Root) {
    $register = Load-Json (Get-RepoPath $Root ".agent/maps/source-register.json")
    foreach ($entry in $register.PSObject.Properties) {
        if (-not (Test-Path -LiteralPath (Get-RepoPath $Root $entry.Name) -PathType Leaf)) { Fail "source register artifact is missing: $($entry.Name)" }
        foreach ($source in @($entry.Value)) {
            if (-not (Test-Path -LiteralPath (Get-RepoPath $Root $source) -PathType Leaf)) { Fail "source register path is missing: $source" }
        }
    }
}

function Assert-SourceLock([string]$Root) {
    $lock = Load-Json (Get-RepoPath $Root ".agent/state/source-lock.json")
    if ($lock.schema_version -ne "1.0.0" -or $lock.algorithm -ne "SHA256") { Fail "source lock schema or algorithm mismatch" }
    $expected = @("Cargo.toml", "docs/FOUNDATION_DECISIONS.md", "docs/HOSTED_PARSER.md", "docs/RISK_REGISTER.md", "docs/SECURITY_AND_OPERATIONS.md", "docs/archive/nutrition_backend_blueprint_v1.0/00_README.md", "docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md", "docs/archive/nutrition_backend_blueprint_v1.0/13_IMPLEMENTATION_CHECKLIST.md")
    $sources = @($lock.sources)
    if ($sources.Count -ne $expected.Count) { Fail "source lock must contain exactly eight sources" }
    for ($index = 0; $index -lt $expected.Count; $index++) {
        if ($sources[$index].path -ne $expected[$index]) { Fail "source lock path order/list mismatch at index $index" }
        $fullPath = Get-RepoPath $Root $expected[$index]
        if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) { Fail "source lock source is missing: $($expected[$index])" }
        $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $fullPath).Hash.ToLowerInvariant()
        if ($actualHash -ne $sources[$index].sha256.ToLowerInvariant()) { Fail "stale source hash: $($expected[$index])" }
    }
}

function Assert-Template([string]$Root) {
    $template = Load-Json (Get-RepoPath $Root ".agent/templates/task-packet.example.json")
    if (@($template.decision_points).Count -ne 0) { Fail "task packet template decision_points must be empty" }
    if (@($template.verification | Where-Object gate -eq "agent-runner-self-test").Count -ne 1) { Fail "task packet template must declare agent-runner-self-test" }
    $verificationReport = Load-Json (Get-RepoPath $Root ".agent/templates/verification-report.example.json")
    if ($verificationReport.schema_version -ne "2.0.0") { Fail "verification report template schema_version mismatch" }
    $evidence = Load-Json (Get-RepoPath $Root ".agent/templates/external-evidence.example.json")
    if ($evidence.schema_version -ne "1.0.0") { Fail "external evidence template schema_version mismatch" }
}

function Assert-ReportContracts([string]$Root) {
    $verification = Load-Json (Get-RepoPath $Root ".agent/contracts/verification-report.schema.json")
    $implementation = Load-Json (Get-RepoPath $Root ".agent/contracts/implementation-report.schema.json")
    $external = Load-Json (Get-RepoPath $Root ".agent/contracts/external-evidence.schema.json")
    if (@($verification.properties.schema_version.const) -ne "2.0.0") { Fail "verification report contract must be 2.0.0" }
    if (@($implementation.properties.schema_version.const) -ne "1.1.0") { Fail "implementation report contract must be 1.1.0" }
    if (@($external.properties.schema_version.const) -ne "1.0.0") { Fail "external evidence contract must be 1.0.0" }
}

function Assert-Impacts($Packet, [string[]]$IntendedPaths, [string[]]$ActualPaths = @()) {
    $impacts = Require-Property $Packet "impacts" "task packet"
    Assert-ExactProperties $impacts @("runtime_behavior", "domain_behavior", "api", "database", "dependencies", "behavior_versions") "task packet impacts"
    foreach ($name in @("runtime_behavior", "domain_behavior", "api", "database", "dependencies", "behavior_versions")) {
        $value = Require-Property $impacts $name "task packet impacts"
        if ($value -notin @("none", "specified_change")) { Fail "invalid impact value for $name" }
    }
    if (@($impacts.PSObject.Properties | Where-Object { $_.Value -eq "specified_change" }).Count -gt 0 -and -not (Has-Property $Packet "impact_spec")) {
        Fail "specified_change impact requires impact_spec"
    }
    $normalized = @($IntendedPaths | ForEach-Object { Normalize-RepoPath $_ })
    $actual = @($ActualPaths | ForEach-Object { Normalize-RepoPath $_ } | Where-Object { $_ -ne "" } | Sort-Object -Unique)
    if ($impacts.dependencies -eq "none" -and @($normalized | Where-Object { $_ -eq "Cargo.toml" -or $_ -eq "Cargo.lock" }).Count -gt 0) { Fail "dependency files changed while dependencies impact is none" }
    if ($impacts.database -eq "none" -and @($normalized | Where-Object { Test-GlobMatch $_ "migrations/**" }).Count -gt 0) { Fail "migration changed while database impact is none" }
    if ($impacts.behavior_versions -eq "none" -and @($normalized | Where-Object { $_ -match '(?i)(behavior.?version|version.?vector|parser.?schema.?version|calculation.?engine.?version|docs/releases/)' }).Count -gt 0) { Fail "behavior-version path changed while behavior_versions impact is none" }
    if ($impacts.dependencies -eq "none" -and @($actual | Where-Object { $_ -eq "Cargo.toml" -or $_ -eq "Cargo.lock" }).Count -gt 0) { Fail "actual dependency impact mismatch: $($actual -join ', ')" }
    if ($impacts.database -eq "none" -and @($actual | Where-Object { Test-GlobMatch $_ "migrations/**" }).Count -gt 0) { Fail "actual database impact mismatch: $($actual -join ', ')" }
    if ($impacts.behavior_versions -eq "none" -and @($actual | Where-Object { $_ -match '(?i)(behavior.?version|version.?vector|parser.?schema.?version|calculation.?engine.?version|docs/releases/)' }).Count -gt 0) { Fail "actual behavior-version impact mismatch: $($actual -join ', ')" }
}

function Assert-VerificationRegistry([string]$Root) {
    $registry = Load-Json (Get-RepoPath $Root ".agent/maps/verification-map.json")
    Assert-ExactProperties $registry @("schema_version", "gates") "verification registry"
    if ($registry.schema_version -ne "2.1.0") { Fail "verification registry schema_version must be 2.1.0" }
    $gates = @((Require-Property $registry "gates" "verification registry"))
    if ($gates.Count -eq 0) { Fail "verification registry gates list is empty" }
    $names = @()
    $allowedKinds = @("control-script", "target-script", "native", "json-parse", "external-evidence")
    foreach ($gate in $gates) {
        Assert-ExactProperties $gate @("name", "kind", "script", "arguments", "program", "paths", "evidence_kind", "display_command", "target_root_argument") "verification registry gate"
        if ($gate.name -isnot [string]) { Fail "verification registry gate name must be a string" }
        if ($gate.kind -isnot [string]) { Fail "verification registry gate kind must be a string" }
        $name = [string](Require-Property $gate "name" "verification registry gate")
        $kind = [string](Require-Property $gate "kind" "verification registry gate $name")
        if ([string]::IsNullOrWhiteSpace($name)) { Fail "verification registry gate name is blank" }
        if ($name -in $names) { Fail "duplicate registry gate name: $name" }
        $names += $name
        if ($kind -notin $allowedKinds) { Fail "unknown registry gate kind: $kind" }
        $displayCommand = Require-Property $gate "display_command" "verification registry gate $name"
        if ($displayCommand -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$displayCommand)) { Fail "registry gate '$name' display_command must be a non-empty string" }
        switch ($kind) {
            "control-script" {
                [void](Require-Property $gate "script" "registry gate $name")
                [void](Require-Property $gate "arguments" "registry gate $name")
                if ($gate.script -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$gate.script)) { Fail "registry control-script '$name' script must be a non-empty string" }
                if ($gate.arguments -is [string] -or $gate.arguments -isnot [System.Collections.IEnumerable]) { Fail "registry control-script '$name' arguments must be an array" }
                if (@($gate.arguments | Where-Object { $_ -isnot [string] }).Count -gt 0) { Fail "registry control-script '$name' arguments must be strings" }
                if ((Has-Property $gate "program") -or (Has-Property $gate "paths") -or (Has-Property $gate "evidence_kind")) { Fail "registry control-script '$name' has invalid kind fields" }
                if ((Has-Property $gate "target_root_argument") -and ($gate.target_root_argument -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$gate.target_root_argument))) { Fail "registry control-script '$name' target_root_argument must be a non-empty string" }
            }
            "target-script" {
                [void](Require-Property $gate "script" "registry gate $name")
                [void](Require-Property $gate "arguments" "registry gate $name")
                if ($gate.script -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$gate.script)) { Fail "registry target-script '$name' script must be a non-empty string" }
                if ($gate.arguments -is [string] -or $gate.arguments -isnot [System.Collections.IEnumerable]) { Fail "registry target-script '$name' arguments must be an array" }
                if (@($gate.arguments | Where-Object { $_ -isnot [string] }).Count -gt 0) { Fail "registry target-script '$name' arguments must be strings" }
                if ((Has-Property $gate "program") -or (Has-Property $gate "paths") -or (Has-Property $gate "evidence_kind") -or (Has-Property $gate "target_root_argument")) { Fail "registry target-script '$name' has invalid kind fields" }
            }
            "native" {
                [void](Require-Property $gate "program" "registry gate $name")
                [void](Require-Property $gate "arguments" "registry gate $name")
                if ($gate.program -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$gate.program)) { Fail "registry native '$name' program must be a non-empty string" }
                if ($gate.arguments -is [string] -or $gate.arguments -isnot [System.Collections.IEnumerable]) { Fail "registry native '$name' arguments must be an array" }
                if (@($gate.arguments | Where-Object { $_ -isnot [string] }).Count -gt 0) { Fail "registry native '$name' arguments must be strings" }
                if ((Has-Property $gate "script") -or (Has-Property $gate "paths") -or (Has-Property $gate "evidence_kind") -or (Has-Property $gate "target_root_argument")) { Fail "registry native '$name' has invalid kind fields" }
            }
            "json-parse" {
                [void](Require-Property $gate "paths" "registry gate $name")
                if ($gate.paths -is [string] -or $gate.paths -isnot [System.Collections.IEnumerable]) { Fail "registry json-parse '$name' paths must be an array" }
                if (@($gate.paths).Count -eq 0 -or @($gate.paths | Where-Object { $_ -isnot [string] -or [string]::IsNullOrWhiteSpace($_) }).Count -gt 0) { Fail "registry json-parse '$name' paths must be non-empty strings" }
                if ((Has-Property $gate "script") -or (Has-Property $gate "arguments") -or (Has-Property $gate "program") -or (Has-Property $gate "evidence_kind") -or (Has-Property $gate "target_root_argument")) { Fail "registry json-parse '$name' has invalid kind fields" }
            }
            "external-evidence" {
                [void](Require-Property $gate "evidence_kind" "registry gate $name")
                if ($gate.evidence_kind -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$gate.evidence_kind)) { Fail "registry external-evidence '$name' evidence_kind must be a non-empty string" }
                if ((Has-Property $gate "script") -or (Has-Property $gate "arguments") -or (Has-Property $gate "program") -or (Has-Property $gate "paths") -or (Has-Property $gate "target_root_argument")) { Fail "registry external-evidence '$name' has invalid kind fields" }
            }
        }
    }
    $runnerGate = @($gates | Where-Object { $_.name -eq "agent-runner-self-test" })
    if ($runnerGate.Count -ne 1 -or $runnerGate[0].kind -ne "control-script" -or $runnerGate[0].script -ne "scripts/run-agent-verification.ps1") { Fail "agent-runner-self-test registry gate is invalid" }
    $foundationGate = @($gates | Where-Object { $_.name -eq "foundation-verify" })
    if ($foundationGate.Count -ne 1 -or $foundationGate[0].kind -ne "target-script") { Fail "foundation-verify must be a target-script" }
    $postgresGate = @($gates | Where-Object { $_.name -eq "postgres-verify" })
    if ($postgresGate.Count -ne 1 -or $postgresGate[0].kind -ne "target-script") { Fail "postgres-verify must be a target-script" }
    return $names
}

function Assert-VerificationGates([string]$Root, $Packet, $Profile) {
    $knownGates = @(Assert-VerificationRegistry $Root)
    if ($knownGates.Count -eq 0) { Fail "verification map has no gates" }
    $entries = @($Packet.verification)
    $declaredNames = @()
    foreach ($entry in $entries) {
        Assert-ExactProperties $entry @("gate", "required") "task packet verification entry"
        [void](Require-Property $entry "gate" "task packet verification entry")
        [void](Require-Property $entry "required" "task packet verification entry")
        if ([string]::IsNullOrWhiteSpace([string]$entry.gate)) { Fail "declared verification gate name is blank" }
        if ($entry.required -isnot [bool]) { Fail "verification gate '$($entry.gate)' required must be boolean" }
        if ($entry.gate -notin $knownGates) { Fail "unknown declared verification gate: $($entry.gate)" }
        if ($entry.gate -in $declaredNames) { Fail "duplicate packet gate declaration: $($entry.gate)" }
        $declaredNames += [string]$entry.gate
    }
    $requiredGates = @($Profile.mandatory_verification_gates | ForEach-Object { [string]$_ })
    foreach ($gate in $requiredGates) {
        $matching = @($entries | Where-Object { $_.gate -eq $gate })
        if ($matching.Count -eq 0) { Fail "missing mandatory profile gate: $gate" }
        if (@($matching | Where-Object { $_.required -eq $true }).Count -eq 0) { Fail "mandatory profile gate is not required: $gate" }
    }
}

function Assert-TaskPacketObject($Packet, [string]$Root, $Profiles) {
    Assert-ExactProperties $Packet @("schema_version", "task_id", "objective", "decision_authority", "executor_role", "context_profile", "required_baseline_commit", "allowed_paths", "forbidden_paths", "create_files", "modify_files", "delete_files", "implementation_sequence", "decision_points", "impacts", "impact_spec", "acceptance_criteria", "verification", "escalation_conditions", "completion_report_required") "task packet"
    foreach ($name in @("schema_version", "task_id", "objective", "decision_authority", "executor_role", "context_profile", "required_baseline_commit", "allowed_paths", "forbidden_paths", "create_files", "modify_files", "delete_files", "implementation_sequence", "decision_points", "impacts", "acceptance_criteria", "verification", "escalation_conditions", "completion_report_required")) {
        [void](Require-Property $Packet $name "task packet")
    }
    if ($Packet.schema_version -ne "1.1.0" -or $Packet.decision_authority -ne "architect" -or $Packet.executor_role -ne "implementation_only" -or $Packet.completion_report_required -ne $true) { Fail "task packet identity fields are invalid" }
    if ([string]::IsNullOrWhiteSpace($Packet.task_id) -or [string]::IsNullOrWhiteSpace($Packet.objective)) { Fail "task packet task_id/objective must be non-empty" }
    if ($Packet.required_baseline_commit -isnot [string] -or $Packet.required_baseline_commit -notmatch '^[0-9a-fA-F]{40}$') { Fail "task packet required_baseline_commit must be a 40-character SHA" }
    Assert-StringArray $Packet.allowed_paths "task packet allowed_paths" $true | Out-Null
    Assert-StringArray $Packet.forbidden_paths "task packet forbidden_paths" | Out-Null
    Assert-StringArray $Packet.create_files "task packet create_files" | Out-Null
    Assert-Array $Packet.modify_files "task packet modify_files" | Out-Null
    Assert-StringArray $Packet.delete_files "task packet delete_files" | Out-Null
    Assert-StringArray $Packet.implementation_sequence "task packet implementation_sequence" $true | Out-Null
    Assert-Array $Packet.decision_points "task packet decision_points" | Out-Null
    if (@($Packet.decision_points).Count -ne 0) { Fail "task packet decision_points must be empty" }
    Assert-StringArray $Packet.acceptance_criteria "task packet acceptance_criteria" $true | Out-Null
    Assert-StringArray $Packet.escalation_conditions "task packet escalation_conditions" $true | Out-Null
    Assert-Array $Packet.verification "task packet verification" $true | Out-Null
    $profile = @($Profiles | Where-Object name -eq $Packet.context_profile)
    if ($profile.Count -ne 1) { Fail "unknown or ambiguous context profile: $($Packet.context_profile)" }
    $allowed = @($Packet.allowed_paths | ForEach-Object { Normalize-RepoPath $_ })
    $forbidden = @($Packet.forbidden_paths | ForEach-Object { Normalize-RepoPath $_ })
    if ($allowed.Count -eq 0) { Fail "task packet allowed_paths must be non-empty" }
    if (@($allowed | Where-Object { $_ -in $forbidden }).Count -gt 0) { Fail "task packet allowed and forbidden paths overlap" }
    $intended = @($Packet.create_files | ForEach-Object { Normalize-RepoPath $_ })
    $deletePaths = @($Packet.delete_files | ForEach-Object { Normalize-RepoPath $_ })
    foreach ($path in $Packet.create_files) { if ([string]::IsNullOrWhiteSpace([string]$path)) { Fail "task packet create_files contains a blank path" } }
    foreach ($path in $Packet.delete_files) { if ([string]::IsNullOrWhiteSpace([string]$path)) { Fail "task packet delete_files contains a blank path" } }
    if (@($Packet.create_files | Where-Object { $_ -isnot [string] }).Count -gt 0 -or @($Packet.delete_files | Where-Object { $_ -isnot [string] }).Count -gt 0) { Fail "task packet create_files/delete_files must contain strings" }
    foreach ($modification in @($Packet.modify_files)) {
        Assert-ExactProperties $modification @("path", "changes") "task packet modify_files item"
        [void](Require-Property $modification "path" "task packet modify_files item")
        [void](Require-Property $modification "changes" "task packet modify_files item")
        if ($modification.path -isnot [string] -or [string]::IsNullOrWhiteSpace([string]$modification.path)) { Fail "task packet modify_files item path must be a non-empty string" }
        Assert-StringArray $modification.changes "task packet modify_files item changes" $true | Out-Null
        $intended += Normalize-RepoPath $modification.path
    }
    $modifyPaths = @($Packet.modify_files | ForEach-Object { Normalize-RepoPath $_.path })
    $allDeclared = @($intended + $deletePaths)
    foreach ($path in @($allDeclared | Where-Object { $_ -ne "" })) {
        if (@($allowed | Where-Object { Test-GlobMatch $path $_ }).Count -eq 0) { Fail "intended path outside packet allowed_paths: $path" }
        if (@($forbidden | Where-Object { Test-GlobMatch $path $_ }).Count -gt 0) { Fail "intended path matches forbidden path: $path" }
    }
    $declaredSets = @(
        @{Name = "create"; Paths = @($Packet.create_files | ForEach-Object { Normalize-RepoPath $_ })},
        @{Name = "modify"; Paths = $modifyPaths},
        @{Name = "delete"; Paths = $deletePaths}
    )
    $seenDeclared = @{}
    foreach ($set in $declaredSets) {
        foreach ($path in $set.Paths) {
            if ([string]::IsNullOrWhiteSpace($path)) { Fail "task packet $($set.Name)_files contains a blank path" }
            if ($seenDeclared.ContainsKey($path)) { Fail "declared change sets overlap at path: $path" }
            $seenDeclared[$path] = $set.Name
        }
    }
    Assert-VerificationGates $Root $Packet $profile[0]
    Assert-Impacts $Packet $intended
    $profileAllowed = @($profile[0].allowed_path_patterns | ForEach-Object { Normalize-RepoPath $_ })
    foreach ($path in $allowed) {
        if ($path.Contains("*") -or $path.Contains("?")) {
            if ($path -notin $profileAllowed) { Fail "task allowed pattern is not present in profile allowlist: $path" }
        }
        elseif (@($profileAllowed | Where-Object { Test-GlobMatch $path $_ }).Count -eq 0) {
            Fail "task allowed path is outside profile allowlist: $path"
        }
    }
    foreach ($path in $intended) {
        if (@($forbidden | Where-Object { Test-GlobMatch $path $_ }).Count -gt 0) { Fail "intended task path matches forbidden path: $path" }
    }
    return [pscustomobject]@{ Create = @($declaredSets[0].Paths | Sort-Object -Unique); Modify = @($declaredSets[1].Paths | Sort-Object -Unique); Delete = @($declaredSets[2].Paths | Sort-Object -Unique) }
}

function Invoke-GitCommand([string]$Root, [string[]]$Arguments) {
    $output = @(& git -C $Root @Arguments)
    if ($LASTEXITCODE -ne 0) { Fail "git command failed: git -C $Root $($Arguments -join ' ')" }
    return @($output | ForEach-Object { [string]$_ })
}

function Assert-TaskBaseline([string]$Root, $Packet) {
    $baseline = [string]$Packet.required_baseline_commit
    if ($baseline -notmatch '^[0-9a-fA-F]{40}$') { Fail "BLOCKED_BASELINE_DRIFT: required_baseline_commit is not a 40-character SHA" }
    $previousNativePreference = $PSNativeCommandUseErrorActionPreference
    $PSNativeCommandUseErrorActionPreference = $false
    try {
        & git -C $Root cat-file -e "$($baseline)^{commit}" 2>$null
        $baselineExitCode = $LASTEXITCODE
        $head = @(& git -C $Root rev-parse --verify "HEAD^{commit}" 2>$null) | Select-Object -First 1
        $headExitCode = $LASTEXITCODE
        if ($baselineExitCode -eq 0 -and $headExitCode -eq 0 -and -not [string]::IsNullOrWhiteSpace([string]$head)) {
            & git -C $Root merge-base --is-ancestor $baseline $head 2>$null
            $ancestorExitCode = $LASTEXITCODE
        }
        else {
            $ancestorExitCode = 1
        }
    }
    finally {
        $PSNativeCommandUseErrorActionPreference = $previousNativePreference
    }
    if ($baselineExitCode -ne 0) { Fail "BLOCKED_BASELINE_DRIFT: baseline commit is not available locally" }
    if ($headExitCode -ne 0 -or [string]::IsNullOrWhiteSpace([string]$head)) { Fail "BLOCKED_BASELINE_DRIFT: HEAD is not a commit" }
    if ($ancestorExitCode -ne 0) { Fail "BLOCKED_BASELINE_DRIFT: baseline is not an ancestor of HEAD" }
    return [string]$head
}

function Get-ActualTaskChanges([string]$Root, [string]$Baseline) {
    $provenance = Get-ActualTaskProvenance $Root $Baseline
    return @($provenance.Keys | Sort-Object)
}

function Get-ActualTaskProvenance([string]$Root, [string]$Baseline) {
    $sources = [ordered]@{
        committed = @(Invoke-GitCommand $Root @("diff", "--name-only", "--no-renames", "$Baseline..HEAD"))
        staged = @(Invoke-GitCommand $Root @("diff", "--cached", "--name-only", "--no-renames"))
        unstaged = @(Invoke-GitCommand $Root @("diff", "--name-only", "--no-renames"))
        untracked = @(Invoke-GitCommand $Root @("ls-files", "--others", "--exclude-standard", "--full-name"))
    }
    $byPath = @{}
    foreach ($source in $sources.Keys) {
        foreach ($rawPath in $sources[$source]) {
            $path = Normalize-RepoPath ([string]$rawPath)
            if ([string]::IsNullOrWhiteSpace($path)) { continue }
            if (-not $byPath.ContainsKey($path)) { $byPath[$path] = @() }
            if ($source -notin $byPath[$path]) { $byPath[$path] += $source }
        }
    }
    return $byPath
}

function Test-GitPathAtCommit([string]$Root, [string]$Commit, [string]$Path) {
    $previousNativePreference = $PSNativeCommandUseErrorActionPreference
    $PSNativeCommandUseErrorActionPreference = $false
    try {
        & git -C $Root cat-file -e "$($Commit):$($Path.Replace('\', '/'))" 2>$null
        return $LASTEXITCODE -eq 0
    }
    finally {
        $PSNativeCommandUseErrorActionPreference = $previousNativePreference
    }
}

function Get-ActualTaskRecords([string]$Root, [string]$Baseline) {
    $paths = @(Get-ActualTaskChanges $Root $Baseline)
    $provenance = Get-ActualTaskProvenance $Root $Baseline
    $records = @()
    foreach ($path in $paths) {
        $baselineExists = Test-GitPathAtCommit $Root $Baseline $path
        $currentExists = Test-Path -LiteralPath (Get-RepoPath $Root $path)
        if (-not $baselineExists -and $currentExists) {
            $type = "create"
        }
        elseif ($baselineExists -and $currentExists) {
            $type = "modify"
        }
        elseif ($baselineExists -and -not $currentExists) {
            $type = "delete"
        }
        else {
            Fail "transient changed path cannot be classified: $path"
        }
        $records += [pscustomobject]@{ Path = $path; Type = $type; Provenance = @($provenance[$path] | Sort-Object); BaselineExists = $baselineExists; CurrentExists = $currentExists }
    }
    return $records
}

function Assert-ExactChangeSet([string]$Name, [string[]]$Actual, [string[]]$Declared) {
    $actualSet = @($Actual | ForEach-Object { Normalize-RepoPath $_ } | Where-Object { $_ -ne "" } | Sort-Object -Unique)
    $declaredSet = @($Declared | ForEach-Object { Normalize-RepoPath $_ } | Where-Object { $_ -ne "" } | Sort-Object -Unique)
    $unexpected = @($actualSet | Where-Object { $_ -notin $declaredSet })
    $missing = @($declaredSet | Where-Object { $_ -notin $actualSet })
    if ($unexpected.Count -gt 0) { Fail "unexpected actual $Name path(s): $($unexpected -join ', ')" }
    if ($missing.Count -gt 0) { Fail "missing declared $Name path(s): $($missing -join ', ')" }
}

function Assert-DeclaredChangeTypes([string]$Root, [string]$Baseline, $Declared) {
    foreach ($path in $Declared.Create) {
        if (Test-GitPathAtCommit $Root $Baseline $path) { Fail "declared create path exists at baseline: $path" }
    }
    foreach ($path in $Declared.Modify) {
        if (-not (Test-GitPathAtCommit $Root $Baseline $path)) { Fail "declared modify path is absent at baseline: $path" }
    }
    foreach ($path in $Declared.Delete) {
        if (-not (Test-GitPathAtCommit $Root $Baseline $path)) { Fail "declared delete path is absent at baseline: $path" }
    }
}

function Assert-ExactTaskChanges([string]$Root, [string]$Baseline, $Declared, [object[]]$Records) {
    Assert-DeclaredChangeTypes $Root $Baseline $Declared
    $declaredByPath = @{}
    foreach ($path in $Declared.Create) { $declaredByPath[(Normalize-RepoPath $path)] = "create" }
    foreach ($path in $Declared.Modify) { $declaredByPath[(Normalize-RepoPath $path)] = "modify" }
    foreach ($path in $Declared.Delete) { $declaredByPath[(Normalize-RepoPath $path)] = "delete" }
    foreach ($record in $Records) {
        $path = Normalize-RepoPath $record.Path
        if ($declaredByPath.ContainsKey($path) -and $declaredByPath[$path] -ne $record.Type) {
            Fail "wrong change type for path: $path declared $($declaredByPath[$path]) but actual $($record.Type)"
        }
    }
    Assert-ExactChangeSet "create" @($Records | Where-Object Type -eq "create" | ForEach-Object Path) $Declared.Create
    Assert-ExactChangeSet "modify" @($Records | Where-Object Type -eq "modify" | ForEach-Object Path) $Declared.Modify
    Assert-ExactChangeSet "delete" @($Records | Where-Object Type -eq "delete" | ForEach-Object Path) $Declared.Delete
}

function Write-TaskStateSnapshot([string]$Path, $Packet, [string]$Head, [object[]]$Records) {
    $state = [ordered]@{
        task_id = [string]$Packet.task_id
        context_profile = [string]$Packet.context_profile
        baseline_commit = [string]$Packet.required_baseline_commit
        head_commit = [string]$Head
        change_records = @($Records | ForEach-Object {
            [ordered]@{path = [string]$_.Path; type = [string]$_.Type; provenance = @($_.Provenance); baseline_exists = [bool]$_.BaselineExists; current_exists = [bool]$_.CurrentExists}
        })
        packet_gates = @($Packet.verification | ForEach-Object {
            [ordered]@{gate = [string]$_.gate; required = [bool]$_.required}
        })
        impacts = [ordered]@{
            runtime_behavior = [string]$Packet.impacts.runtime_behavior
            domain_behavior = [string]$Packet.impacts.domain_behavior
            api = [string]$Packet.impacts.api
            database = [string]$Packet.impacts.database
            dependencies = [string]$Packet.impacts.dependencies
            behavior_versions = [string]$Packet.impacts.behavior_versions
        }
    }
    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    $temporary = "$Path.$([guid]::NewGuid().ToString('N')).tmp"
    try {
        $state | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $temporary -Encoding utf8
        Move-Item -Force -LiteralPath $temporary -Destination $Path
    }
    finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -Force -LiteralPath $temporary }
    }
    return $Path
}

function Assert-ChangedScope([string]$Root, $Packet, [string[]]$ActualTaskChanges) {
    $changed = @($ActualTaskChanges | ForEach-Object { Normalize-RepoPath $_ } | Where-Object { $_ -ne "" } | Sort-Object -Unique)
    $allowed = @($Packet.allowed_paths | ForEach-Object { Normalize-RepoPath $_ })
    $forbidden = @($Packet.forbidden_paths | ForEach-Object { Normalize-RepoPath $_ })
    foreach ($path in $changed) {
        if (@($forbidden | Where-Object { Test-GlobMatch $path $_ }).Count -gt 0) { Fail "changed path is forbidden by task packet: $path" }
        if (@($allowed | Where-Object { Test-GlobMatch $path $_ }).Count -eq 0) { Fail "changed path is outside task allowlist: $path" }
    }
    Assert-Impacts $Packet @() $changed
    Write-Output "[PASS] Task changed-path scope: $($changed.Count) tracked changed file(s)."
}

function Assert-Integrity([string]$Root) {
    Assert-RequiredAclFiles $Root
    Assert-JsonArtifacts $Root
    $manifest = Assert-Manifest $Root
    Assert-Budgets $Root $manifest
    $profiles = Assert-Profiles $Root
    Assert-SourceRegister $Root
    Assert-SourceLock $Root
    Assert-VerificationRegistry $Root
    Assert-ReportContracts $Root
    Assert-Template $Root
    return $profiles
}

function New-BaseSelfTestPacket {
    $packet = [pscustomobject][ordered]@{
        schema_version = "1.1.0"
        task_id = "ACL-SELFTEST"
        objective = "Exercise ACL verifier"
        decision_authority = "architect"
        executor_role = "implementation_only"
        context_profile = "agent-context-maintenance"
        required_baseline_commit = "da04e773a214e8f8232db149d1f35f3f0bd61ce1"
        allowed_paths = @(".agent/README.md")
        forbidden_paths = @("crates/**", "migrations/**", "Cargo.toml", "Cargo.lock")
        create_files = @()
        delete_files = @()
        modify_files = @([pscustomobject]@{path = ".agent/README.md"; changes = @("replace exact sentence")})
        implementation_sequence = @("validate", "change", "verify")
        decision_points = @()
        impacts = [pscustomobject][ordered]@{runtime_behavior = "none"; domain_behavior = "none"; api = "none"; database = "none"; dependencies = "none"; behavior_versions = "none"}
        acceptance_criteria = @("scope remains ACL-only")
        verification = @(
            [pscustomobject]@{gate = "acl-self-test"; required = $true},
            [pscustomobject]@{gate = "agent-runner-self-test"; required = $true},
            [pscustomobject]@{gate = "acl-integrity"; required = $true},
            [pscustomobject]@{gate = "foundation-verify"; required = $true}
        )
        escalation_conditions = @("outside allowlist")
        completion_report_required = $true
    }
    return $packet
}

function Set-SelfTestFile([string]$Root, [string]$RelativePath, [string]$Content) {
    $path = Get-RepoPath $Root $RelativePath
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    Set-Content -LiteralPath $path -Value $Content -NoNewline
}

function New-SelfTestGitRepository([string]$SourceRoot, [hashtable]$BaselineFiles = @{}) {
    $root = Join-Path ([IO.Path]::GetTempPath()) ("agent-context-git-selftest-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $root | Out-Null
    try {
        [void](Invoke-GitCommand $root @("init", "--quiet"))
        [void](Invoke-GitCommand $root @("config", "user.name", "ACL P09 SelfTest"))
        [void](Invoke-GitCommand $root @("config", "user.email", "acl-p09-selftest@example.invalid"))
        Set-SelfTestFile $root "selftest-ignore" ""
        [void](Invoke-GitCommand $root @("config", "core.excludesFile", (Get-RepoPath $root "selftest-ignore")))
        Set-SelfTestFile $root ".agent/README.md" "temporary ACL fixture"
        $mapSource = Get-RepoPath $SourceRoot ".agent/maps/verification-map.json"
        $mapDestination = Get-RepoPath $root ".agent/maps/verification-map.json"
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $mapDestination) | Out-Null
        Copy-Item -Force -LiteralPath $mapSource -Destination $mapDestination
        $manifestSource = Get-RepoPath $SourceRoot ".agent/manifest.json"
        $manifestDestination = Get-RepoPath $root ".agent/manifest.json"
        Copy-Item -Force -LiteralPath $manifestSource -Destination $manifestDestination
        foreach ($entry in $BaselineFiles.GetEnumerator()) {
            Set-SelfTestFile $root ([string]$entry.Key) ([string]$entry.Value)
        }
        [void](Invoke-GitCommand $root @("add", "--all"))
        [void](Invoke-GitCommand $root @("commit", "--quiet", "-m", "P09 self-test baseline"))
        $baseline = [string](@(Invoke-GitCommand $root @("rev-parse", "HEAD")) | Select-Object -First 1)
        return [pscustomobject]@{ Root = $root; Baseline = $baseline }
    }
    catch {
        if (Test-Path -LiteralPath $root) { Remove-Item -Recurse -Force -LiteralPath $root }
        throw
    }
}

function New-RealGitSelfTestPacket([string]$Baseline) {
    $packet = New-BaseSelfTestPacket
    $packet.required_baseline_commit = $Baseline
    $packet.allowed_paths = @(".agent/**")
    $packet.forbidden_paths = @("crates/**", "migrations/**", "Cargo.toml", "Cargo.lock", "docs/releases/**")
    $packet.create_files = @()
    $packet.modify_files = @()
    return $packet
}

function Assert-SelfTestExpected([string]$Name, [bool]$ExpectedPass, [scriptblock]$Action, [string]$ExpectedReason = "") {
    $observedPass = $false
    $failureText = ""
    try {
        & $Action
        $observedPass = $true
    }
    catch {
        $failureText = [string]$_.Exception.Message
    }
    if ($observedPass -ne $ExpectedPass) {
        Fail "self-test case '$Name' expected $([string]$ExpectedPass) but observed $([string]$observedPass): $failureText"
    }
    if (-not $ExpectedPass -and -not [string]::IsNullOrWhiteSpace($ExpectedReason) -and $failureText.IndexOf($ExpectedReason, [StringComparison]::OrdinalIgnoreCase) -lt 0) {
        Fail "self-test case '$Name' expected failure reason '$ExpectedReason' but observed: $failureText"
    }
    if ($ExpectedPass) {
        Write-Output "[PASS] Self-test: $Name"
    }
    else {
        Write-Output "[PASS] Self-test: $Name :: $failureText"
    }
}

function Invoke-RealGitScopeScenario([string]$SourceRoot, [string]$ProfilesRoot, [string]$Name, [scriptblock]$Setup, [bool]$ExpectedPass, [hashtable]$BaselineFiles = @{}) {
    $repository = New-SelfTestGitRepository $SourceRoot $BaselineFiles
    try {
        $packet = New-RealGitSelfTestPacket $repository.Baseline
        & $Setup $repository.Root $packet | Out-Null
        Assert-SelfTestExpected $Name $ExpectedPass {
            [void](Assert-TaskBaseline $repository.Root $packet)
            $profiles = Assert-Profiles $ProfilesRoot
            [void](Assert-TaskPacketObject $packet $ProfilesRoot $profiles)
            $actual = @(Get-ActualTaskChanges $repository.Root $packet.required_baseline_commit)
            Assert-ChangedScope $repository.Root $packet $actual
        }
    }
    finally {
        if (Test-Path -LiteralPath $repository.Root) { Remove-Item -Recurse -Force -LiteralPath $repository.Root }
    }
}

function Invoke-PacketObjectScenario([string]$SourceRoot, [string]$Name, [scriptblock]$Setup, [bool]$ExpectedPass) {
    $packet = New-BaseSelfTestPacket
    & $Setup $packet | Out-Null
    Assert-SelfTestExpected $Name $ExpectedPass {
        $profiles = Assert-Profiles $SourceRoot
        [void](Assert-TaskPacketObject $packet $SourceRoot $profiles)
    }
}

function Invoke-ActualImpactScenario([string]$SourceRoot, [string]$Name, [string]$ChangedPath, [string]$Content, [bool]$ExpectedPass) {
    $repository = New-SelfTestGitRepository $SourceRoot
    try {
        Set-SelfTestFile $repository.Root $ChangedPath $Content
        $packet = New-RealGitSelfTestPacket $repository.Baseline
        Assert-SelfTestExpected $Name $ExpectedPass {
            [void](Assert-TaskBaseline $repository.Root $packet)
            $actual = @(Get-ActualTaskChanges $repository.Root $packet.required_baseline_commit)
            Assert-Impacts $packet @() $actual
        }
    }
    finally {
        if (Test-Path -LiteralPath $repository.Root) { Remove-Item -Recurse -Force -LiteralPath $repository.Root }
    }
}

function Invoke-P09SelfTests([string]$Root, $Cases) {
    $requiredNames = @(
        "valid_complete_task_delta", "outside_allowlist_unstaged", "outside_allowlist_staged", "outside_allowlist_untracked", "outside_allowlist_committed_after_baseline", "mixed_allowed_and_forbidden_states", "baseline_commit_missing", "missing_mandatory_profile_gate", "mandatory_gate_marked_not_required", "unknown_declared_verification_gate", "intended_create_outside_allowed_paths", "intended_modify_outside_allowed_paths", "actual_dependency_change_impact_none", "actual_migration_change_impact_none", "actual_behavior_version_change_impact_none", "committed_allowed_change_after_baseline", "staged_allowed_change", "untracked_allowed_change"
    )
    $caseNames = @($Cases | ForEach-Object { [string]$_.name })
    foreach ($name in $requiredNames) {
        if ($name -notin $caseNames) { Fail "P09 self-test case is missing from eval matrix: $name" }
    }

    Invoke-RealGitScopeScenario $Root $Root "valid_complete_task_delta" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/committed.txt" "committed allowed"
        [void](Invoke-GitCommand $repositoryRoot @("add", "--all"))
        [void](Invoke-GitCommand $repositoryRoot @("commit", "--quiet", "-m", "allowed committed change"))
        Set-SelfTestFile $repositoryRoot ".agent/staged.txt" "staged allowed"
        [void](Invoke-GitCommand $repositoryRoot @("add", ".agent/staged.txt"))
        Set-SelfTestFile $repositoryRoot ".agent/README.md" "unstaged allowed"
        Set-SelfTestFile $repositoryRoot ".agent/untracked.txt" "untracked allowed"
    } $true
    Invoke-RealGitScopeScenario $Root $Root "outside_allowlist_unstaged" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot "crates/domain/src/lib.rs" "changed unstaged"
    } $false @{"crates/domain/src/lib.rs" = "baseline tracked file"}
    Invoke-RealGitScopeScenario $Root $Root "outside_allowlist_staged" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot "crates/domain/src/lib.rs" "changed staged"
        [void](Invoke-GitCommand $repositoryRoot @("add", "crates/domain/src/lib.rs"))
    } $false @{"crates/domain/src/lib.rs" = "baseline tracked file"}
    Invoke-RealGitScopeScenario $Root $Root "outside_allowlist_untracked" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot "crates/domain/src/untracked.rs" "untracked forbidden"
    } $false
    Invoke-RealGitScopeScenario $Root $Root "outside_allowlist_committed_after_baseline" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot "crates/domain/src/committed.rs" "committed forbidden"
        [void](Invoke-GitCommand $repositoryRoot @("add", "crates/domain/src/committed.rs"))
        [void](Invoke-GitCommand $repositoryRoot @("commit", "--quiet", "-m", "forbidden committed change"))
    } $false
    Invoke-RealGitScopeScenario $Root $Root "mixed_allowed_and_forbidden_states" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/allowed-staged.txt" "allowed staged"
        [void](Invoke-GitCommand $repositoryRoot @("add", ".agent/allowed-staged.txt"))
        Set-SelfTestFile $repositoryRoot "crates/domain/src/forbidden-untracked.rs" "forbidden untracked"
    } $false

    $missingBaselineRepository = New-SelfTestGitRepository $Root
    try {
        $missingPacket = New-RealGitSelfTestPacket ("1111111111111111111111111111111111111111")
        Assert-SelfTestExpected "baseline_commit_missing" $false {
            [void](Assert-TaskBaseline $missingBaselineRepository.Root $missingPacket)
        }
    }
    finally {
        if (Test-Path -LiteralPath $missingBaselineRepository.Root) { Remove-Item -Recurse -Force -LiteralPath $missingBaselineRepository.Root }
    }

    Invoke-PacketObjectScenario $Root "missing_mandatory_profile_gate" {
        param($packet)
        $packet.verification = @($packet.verification | Where-Object { $_.gate -ne "foundation-verify" })
    } $false
    Invoke-PacketObjectScenario $Root "mandatory_gate_marked_not_required" {
        param($packet)
        ($packet.verification | Where-Object { $_.gate -eq "foundation-verify" })[0].required = $false
    } $false
    Invoke-PacketObjectScenario $Root "unknown_declared_verification_gate" {
        param($packet)
        $packet.verification += [pscustomobject]@{gate = "unknown-gate"; command = "unknown"; required = $true}
    } $false
    Invoke-PacketObjectScenario $Root "intended_create_outside_allowed_paths" {
        param($packet)
        $packet.create_files = @("crates/domain/src/new.rs")
    } $false
    Invoke-PacketObjectScenario $Root "intended_modify_outside_allowed_paths" {
        param($packet)
        $packet.modify_files = @([pscustomobject]@{path = "crates/domain/src/lib.rs"; changes = @("outside allowlist")})
    } $false

    Invoke-ActualImpactScenario $Root "actual_dependency_change_impact_none" "Cargo.toml" "actual dependency change" $false
    Invoke-ActualImpactScenario $Root "actual_migration_change_impact_none" "migrations/0002.sql" "actual migration change" $false
    Invoke-ActualImpactScenario $Root "actual_behavior_version_change_impact_none" "docs/releases/p09.md" "actual behavior version change" $false

    Invoke-RealGitScopeScenario $Root $Root "committed_allowed_change_after_baseline" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/committed-allowed.txt" "allowed committed"
        [void](Invoke-GitCommand $repositoryRoot @("add", ".agent/committed-allowed.txt"))
        [void](Invoke-GitCommand $repositoryRoot @("commit", "--quiet", "-m", "allowed committed change"))
    } $true
    Invoke-RealGitScopeScenario $Root $Root "staged_allowed_change" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/staged-allowed.txt" "allowed staged"
        [void](Invoke-GitCommand $repositoryRoot @("add", ".agent/staged-allowed.txt"))
    } $true
    Invoke-RealGitScopeScenario $Root $Root "untracked_allowed_change" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/untracked-allowed.txt" "allowed untracked"
    } $true
}

function New-P10SelfTestPacket([string]$Baseline) {
    $packet = New-BaseSelfTestPacket
    $packet.required_baseline_commit = $Baseline
    $packet.allowed_paths = @(".agent/**")
    $packet.forbidden_paths = @("crates/**", "migrations/**", "Cargo.toml", "Cargo.lock", "docs/releases/**", "deploy/**", "schemas/**", "fixtures/**", "seeds/**", "scripts/verify.ps1", ".agent/state/source-lock.json")
    $packet.create_files = @()
    $packet.modify_files = @()
    $packet.delete_files = @()
    return $packet
}

function Invoke-P10ExactGitScenario([string]$SourceRoot, [string]$Name, [scriptblock]$Setup, [bool]$ExpectedPass, [hashtable]$BaselineFiles = @{}, [string]$ExpectedReason = "") {
    $repository = New-SelfTestGitRepository $SourceRoot $BaselineFiles
    try {
        $packet = New-P10SelfTestPacket $repository.Baseline
        & $Setup $repository.Root $packet | Out-Null
        Assert-SelfTestExpected $Name $ExpectedPass {
            [void](Assert-TaskBaseline $repository.Root $packet)
            $profiles = Assert-Profiles $SourceRoot
            $declared = Assert-TaskPacketObject $packet $repository.Root $profiles
            $records = @(Get-ActualTaskRecords $repository.Root $packet.required_baseline_commit)
            $actual = @($records | ForEach-Object Path)
            Assert-ChangedScope $repository.Root $packet $actual
            Assert-ExactTaskChanges $repository.Root $packet.required_baseline_commit $declared $records
        } $ExpectedReason
    }
    finally {
        if (Test-Path -LiteralPath $repository.Root) { Remove-Item -Recurse -Force -LiteralPath $repository.Root }
    }
}

function Invoke-P10PacketScenario([string]$SourceRoot, [string]$Name, [scriptblock]$Setup, [bool]$ExpectedPass, [string]$ExpectedReason = "") {
    $packet = New-P10SelfTestPacket "da04e773a214e8f8232db149d1f35f3f0bd61ce1"
    & $Setup $packet | Out-Null
    Assert-SelfTestExpected $Name $ExpectedPass {
        $profiles = Assert-Profiles $SourceRoot
        [void](Assert-TaskPacketObject $packet $SourceRoot $profiles)
    } $ExpectedReason
}

function Invoke-P10RegistryScenario([string]$SourceRoot, [string]$Name, [scriptblock]$Setup, [string]$ExpectedReason = "") {
    $repository = New-SelfTestGitRepository $SourceRoot
    try {
        $registryPath = Get-RepoPath $repository.Root ".agent/maps/verification-map.json"
        $registry = Load-Json $registryPath
        & $Setup $registry | Out-Null
        $registry | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $registryPath -Encoding utf8
        Assert-SelfTestExpected $Name $false {
            [void](Assert-VerificationRegistry $repository.Root)
        } $ExpectedReason
    }
    finally {
        if (Test-Path -LiteralPath $repository.Root) { Remove-Item -Recurse -Force -LiteralPath $repository.Root }
    }
}

function Invoke-P10ManifestScenario([string]$SourceRoot, [string]$Name, [string]$Property, [string]$ExpectedReason = "") {
    $repository = New-SelfTestGitRepository $SourceRoot
    try {
        $manifestPath = Get-RepoPath $repository.Root ".agent/manifest.json"
        $manifest = Load-Json $manifestPath
        $manifest.$Property = "agent-invalid-release"
        $manifest | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $manifestPath -Encoding utf8
        Assert-SelfTestExpected $Name $false {
            [void](Assert-Manifest $repository.Root)
        } $ExpectedReason
    }
    finally {
        if (Test-Path -LiteralPath $repository.Root) { Remove-Item -Recurse -Force -LiteralPath $repository.Root }
    }
}

function Invoke-P10SelfTests([string]$Root, $Cases) {
    $requiredNames = @(
        "exact_declared_modify_pass", "exact_declared_create_pass", "exact_declared_delete_pass", "undeclared_modify_inside_allowed_glob", "undeclared_create_inside_allowed_glob", "undeclared_delete_inside_allowed_glob", "declared_modify_but_deleted", "declared_create_but_existing", "declared_modify_but_created", "declared_delete_but_modified", "declared_file_skipped", "declared_sets_overlap", "duplicate_declared_path", "rename_delete_plus_create", "rename_only_modify", "transient_committed_create_then_removed", "legacy_verification_command_rejected", "duplicate_packet_gate_rejected", "duplicate_registry_gate_rejected", "unknown_registry_kind_rejected", "registry_control_script_missing_script", "registry_native_missing_program", "registry_json_parse_missing_paths", "manifest_contract_release_mismatch", "manifest_verifier_release_mismatch", "manifest_registry_release_mismatch"
    )
    $caseNames = @($Cases | ForEach-Object { [string]$_.name })
    $expectedReasons = @{}
    foreach ($case in $Cases) {
        if (Has-Property $case "expected_reason") { $expectedReasons[[string]$case.name] = [string]$case.expected_reason }
    }
    foreach ($name in $requiredNames) {
        if ($name -notin $caseNames) { Fail "P10A self-test case is missing from eval matrix: $name" }
    }

    Invoke-P10ExactGitScenario $Root "exact_declared_modify_pass" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/p10-modify.txt" "changed"
        $packet.modify_files = @([pscustomobject]@{path = ".agent/p10-modify.txt"; changes = @("P10A exact modify")})
    } $true @{".agent/p10-modify.txt" = "baseline"}
    Invoke-P10ExactGitScenario $Root "exact_declared_create_pass" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/p10-create.txt" "created"
        $packet.create_files = @(".agent/p10-create.txt")
    } $true
    Invoke-P10ExactGitScenario $Root "exact_declared_delete_pass" {
        param($repositoryRoot, $packet)
        Remove-Item -LiteralPath (Get-RepoPath $repositoryRoot ".agent/p10-delete.txt")
        $packet.delete_files = @(".agent/p10-delete.txt")
    } $true @{".agent/p10-delete.txt" = "deleted"}
    Invoke-P10ExactGitScenario $Root "undeclared_modify_inside_allowed_glob" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/p10-modify-a.txt" "changed"
        Set-SelfTestFile $repositoryRoot ".agent/p10-modify-b.txt" "unexpected"
        $packet.modify_files = @([pscustomobject]@{path = ".agent/p10-modify-a.txt"; changes = @("declared")})
    } $false @{".agent/p10-modify-a.txt" = "baseline-a"; ".agent/p10-modify-b.txt" = "baseline-b"} $expectedReasons["undeclared_modify_inside_allowed_glob"]
    Invoke-P10ExactGitScenario $Root "undeclared_create_inside_allowed_glob" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/p10-create-a.txt" "created"
        Set-SelfTestFile $repositoryRoot ".agent/p10-create-b.txt" "unexpected"
        $packet.create_files = @(".agent/p10-create-a.txt")
    } $false @{} $expectedReasons["undeclared_create_inside_allowed_glob"]
    Invoke-P10ExactGitScenario $Root "undeclared_delete_inside_allowed_glob" {
        param($repositoryRoot, $packet)
        Remove-Item -LiteralPath (Get-RepoPath $repositoryRoot ".agent/p10-delete-a.txt")
        Remove-Item -LiteralPath (Get-RepoPath $repositoryRoot ".agent/p10-delete-b.txt")
        $packet.delete_files = @(".agent/p10-delete-a.txt")
    } $false @{".agent/p10-delete-a.txt" = "baseline-a"; ".agent/p10-delete-b.txt" = "baseline-b"} $expectedReasons["undeclared_delete_inside_allowed_glob"]
    Invoke-P10ExactGitScenario $Root "declared_modify_but_deleted" {
        param($repositoryRoot, $packet)
        Remove-Item -LiteralPath (Get-RepoPath $repositoryRoot ".agent/p10-modify.txt")
        $packet.modify_files = @([pscustomobject]@{path = ".agent/p10-modify.txt"; changes = @("declared modify")})
    } $false @{".agent/p10-modify.txt" = "baseline"} $expectedReasons["declared_modify_but_deleted"]
    Invoke-P10ExactGitScenario $Root "declared_create_but_existing" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/p10-existing.txt" "changed"
        $packet.create_files = @(".agent/p10-existing.txt")
    } $false @{".agent/p10-existing.txt" = "baseline"} $expectedReasons["declared_create_but_existing"]
    Invoke-P10ExactGitScenario $Root "declared_modify_but_created" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/p10-new-modify.txt" "created"
        $packet.modify_files = @([pscustomobject]@{path = ".agent/p10-new-modify.txt"; changes = @("declared modify")})
    } $false @{} $expectedReasons["declared_modify_but_created"]
    Invoke-P10ExactGitScenario $Root "declared_delete_but_modified" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/p10-delete-modified.txt" "changed"
        $packet.delete_files = @(".agent/p10-delete-modified.txt")
    } $false @{".agent/p10-delete-modified.txt" = "baseline"} $expectedReasons["declared_delete_but_modified"]
    Invoke-P10ExactGitScenario $Root "declared_file_skipped" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/p10-changed.txt" "changed"
        $packet.modify_files = @(
            [pscustomobject]@{path = ".agent/p10-changed.txt"; changes = @("changed")},
            [pscustomobject]@{path = ".agent/p10-skipped.txt"; changes = @("skipped")}
        )
    } $false @{".agent/p10-changed.txt" = "baseline"; ".agent/p10-skipped.txt" = "baseline"} $expectedReasons["declared_file_skipped"]
    Invoke-P10PacketScenario $Root "declared_sets_overlap" {
        param($packet)
        $packet.create_files = @(".agent/p10-overlap.txt")
        $packet.modify_files = @([pscustomobject]@{path = ".agent/p10-overlap.txt"; changes = @("overlap")})
    } $false $expectedReasons["declared_sets_overlap"]
    Invoke-P10PacketScenario $Root "duplicate_declared_path" {
        param($packet)
        $packet.create_files = @(".agent/p10-duplicate.txt", ".agent/p10-duplicate.txt")
    } $false $expectedReasons["duplicate_declared_path"]
    Invoke-P10ExactGitScenario $Root "rename_delete_plus_create" {
        param($repositoryRoot, $packet)
        Remove-Item -LiteralPath (Get-RepoPath $repositoryRoot ".agent/p10-old.txt")
        Set-SelfTestFile $repositoryRoot ".agent/p10-new.txt" "renamed"
        $packet.create_files = @(".agent/p10-new.txt")
        $packet.delete_files = @(".agent/p10-old.txt")
    } $true @{".agent/p10-old.txt" = "old"}
    Invoke-P10ExactGitScenario $Root "rename_only_modify" {
        param($repositoryRoot, $packet)
        Remove-Item -LiteralPath (Get-RepoPath $repositoryRoot ".agent/p10-old.txt")
        Set-SelfTestFile $repositoryRoot ".agent/p10-new.txt" "renamed"
        $packet.modify_files = @([pscustomobject]@{path = ".agent/p10-old.txt"; changes = @("rename is not modify")})
    } $false @{".agent/p10-old.txt" = "old"} $expectedReasons["rename_only_modify"]
    Invoke-P10ExactGitScenario $Root "transient_committed_create_then_removed" {
        param($repositoryRoot, $packet)
        Set-SelfTestFile $repositoryRoot ".agent/p10-transient.txt" "created then removed"
        [void](Invoke-GitCommand $repositoryRoot @("add", ".agent/p10-transient.txt"))
        [void](Invoke-GitCommand $repositoryRoot @("commit", "--quiet", "-m", "transient create"))
        Remove-Item -LiteralPath (Get-RepoPath $repositoryRoot ".agent/p10-transient.txt")
    } $false @{} $expectedReasons["transient_committed_create_then_removed"]
    Invoke-P10PacketScenario $Root "legacy_verification_command_rejected" {
        param($packet)
        $packet.verification = @([pscustomobject]@{gate = "acl-self-test"; required = $true; command = "legacy command"})
    } $false $expectedReasons["legacy_verification_command_rejected"]
    Invoke-P10PacketScenario $Root "duplicate_packet_gate_rejected" {
        param($packet)
        $packet.verification += [pscustomobject]@{gate = "acl-self-test"; required = $true}
    } $false $expectedReasons["duplicate_packet_gate_rejected"]
    Invoke-P10RegistryScenario $Root "duplicate_registry_gate_rejected" {
        param($registry)
        $registry.gates = @($registry.gates) + @($registry.gates[0])
    } $expectedReasons["duplicate_registry_gate_rejected"]
    Invoke-P10RegistryScenario $Root "unknown_registry_kind_rejected" {
        param($registry)
        $registry.gates[0].kind = "unknown-kind"
    } $expectedReasons["unknown_registry_kind_rejected"]
    Invoke-P10RegistryScenario $Root "registry_control_script_missing_script" {
        param($registry)
        $registry.gates[0].PSObject.Properties.Remove("script")
    } $expectedReasons["registry_control_script_missing_script"]
    Invoke-P10RegistryScenario $Root "registry_native_missing_program" {
        param($registry)
        ($registry.gates | Where-Object name -eq "cargo-fmt").PSObject.Properties.Remove("program")
    } $expectedReasons["registry_native_missing_program"]
    Invoke-P10RegistryScenario $Root "registry_json_parse_missing_paths" {
        param($registry)
        ($registry.gates | Where-Object name -eq "schema-validation").PSObject.Properties.Remove("paths")
    } $expectedReasons["registry_json_parse_missing_paths"]
    Invoke-P10ManifestScenario $Root "manifest_contract_release_mismatch" "contract_release" $expectedReasons["manifest_contract_release_mismatch"]
    Invoke-P10ManifestScenario $Root "manifest_verifier_release_mismatch" "verifier_release" $expectedReasons["manifest_verifier_release_mismatch"]
    Invoke-P10ManifestScenario $Root "manifest_registry_release_mismatch" "verification_registry_release" $expectedReasons["manifest_registry_release_mismatch"]
}

function Invoke-SelfTest([string]$Root) {
    $casesDocument = Load-Json (Get-RepoPath $Root ".agent/evals/context-layer-cases.json")
    $cases = @((Require-Property $casesDocument "cases" "self-test cases"))
    if ($cases.Count -lt 56) { Fail "P10A self-test requires at least fifty-six cases" }
    $tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("agent-context-selftest-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    try {
        Copy-Item -Recurse -Force -LiteralPath (Get-RepoPath $Root ".agent") -Destination (Join-Path $tempRoot ".agent")
        $lockSources = @("Cargo.toml", "docs/FOUNDATION_DECISIONS.md", "docs/HOSTED_PARSER.md", "docs/RISK_REGISTER.md", "docs/SECURITY_AND_OPERATIONS.md", "docs/archive/nutrition_backend_blueprint_v1.0/00_README.md", "docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md", "docs/archive/nutrition_backend_blueprint_v1.0/13_IMPLEMENTATION_CHECKLIST.md")
        foreach ($source in $lockSources) {
            $destination = Get-RepoPath $tempRoot $source
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
            $sourcePath = Get-RepoPath $Root $source
            if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
                Fail "self-test source fixture is missing: $source"
            }
            Copy-Item -Force -LiteralPath $sourcePath -Destination $destination
        }
        $profiles = Assert-Profiles $tempRoot
        $legacyNames = @("valid_context_maintenance_packet", "missing_context_profile", "unknown_context_profile", "non_empty_decision_points", "allowed_and_forbidden_overlap", "dependency_change_declared_none", "migration_change_declared_none", "changed_file_outside_allowlist", "forbidden_runtime_file_for_acl_task", "stale_source_hash", "oversized_agents_md_fixture", "profile_references_missing_file")
        foreach ($case in @($cases | Where-Object { $_.name -in $legacyNames })) {
            $packet = New-BaseSelfTestPacket
            $name = $case.name
            switch ($name) {
                "missing_context_profile" { $packet.context_profile = "" }
                "unknown_context_profile" { $packet.context_profile = "missing-profile" }
                "non_empty_decision_points" { $packet.decision_points = @("choose") }
                "allowed_and_forbidden_overlap" { $packet.forbidden_paths += ".agent/README.md" }
                "dependency_change_declared_none" { $packet.modify_files = @([pscustomobject]@{path = "Cargo.toml"; changes = @("change dependency")}) }
                "migration_change_declared_none" { $packet.modify_files = @([pscustomobject]@{path = "migrations/0002.sql"; changes = @("change schema")}); $packet.impacts.database = "none" }
                "changed_file_outside_allowlist" { $packet.modify_files = @([pscustomobject]@{path = "crates/domain/src/lib.rs"; changes = @("runtime change")}) }
                "forbidden_runtime_file_for_acl_task" { $packet.allowed_paths = @(".agent/**"); $packet.modify_files = @([pscustomobject]@{path = "crates/domain/src/lib.rs"; changes = @("runtime change")}) }
                "stale_source_hash" { Add-Content -LiteralPath (Get-RepoPath $tempRoot "Cargo.toml") -Value "# stale self-test fixture" }
                "oversized_agents_md_fixture" { Set-Content -LiteralPath (Get-RepoPath $tempRoot "AGENTS.md") -Value ("x" * 4097) }
                "profile_references_missing_file" { Remove-Item -LiteralPath (Get-RepoPath $tempRoot ".agent/contexts/verification.md") }
            }
            $taskPath = Get-RepoPath $tempRoot ("case-" + $name + ".json")
            $packet | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $taskPath -Encoding utf8
            $observedPass = $false
            $failureText = ""
            try {
                if ($name -eq "stale_source_hash") {
                    Assert-SourceLock $tempRoot
                }
                elseif ($name -eq "oversized_agents_md_fixture") {
                    Assert-Budgets $tempRoot (Load-Json (Get-RepoPath $tempRoot ".agent/manifest.json"))
                }
                elseif ($name -eq "profile_references_missing_file") {
                    Assert-Profiles $tempRoot
                }
                else {
                    Assert-TaskPacketObject (Load-Json $taskPath) $tempRoot $profiles | Out-Null
                }
                $observedPass = $true
            }
            catch {
                $observedPass = $false
                $failureText = [string]$_.Exception.Message
            }
            $expectedPass = $case.expected -eq "pass"
            if ($observedPass -ne $expectedPass) { Fail "self-test case '$name' expected $($case.expected) but observed $([string]$observedPass): $failureText" }
            Write-Output "[PASS] Self-test: $name"
        }
    }
    finally {
        if (Test-Path -LiteralPath $tempRoot) { Remove-Item -Recurse -Force -LiteralPath $tempRoot }
    }
    Invoke-P09SelfTests $Root $cases
    Invoke-P10SelfTests $Root $cases
    Write-Output "[PASS] All $($cases.Count) ACL self-test cases passed."
}

Push-Location $script:RepoRoot
try {
    if ($SelfTest -and ($TaskPacket -or $TaskStateOutput)) { Fail "-SelfTest cannot be combined with task validation parameters" }
    if ($TaskStateOutput -and -not $TaskPacket) { Fail "-TaskStateOutput requires -TaskPacket" }
    if ($SelfTest) {
        Invoke-SelfTest $script:RepoRoot
        exit 0
    }
    $profiles = Assert-Integrity $script:RepoRoot
    if ($TaskPacket) {
        $packetPath = if ([IO.Path]::IsPathRooted($TaskPacket)) { $TaskPacket } else { Get-RepoPath $script:RepoRoot $TaskPacket }
        $packet = Load-Json $packetPath
        $head = Assert-TaskBaseline $script:RepoRoot $packet
        Write-Output "[PASS] Task baseline commit: $($packet.required_baseline_commit)"
        $declaredChanges = Assert-TaskPacketObject $packet $script:RepoRoot $profiles
        $actualRecords = @(Get-ActualTaskRecords $script:RepoRoot $packet.required_baseline_commit)
        $actualTaskChanges = @($actualRecords | ForEach-Object Path)
        Write-Output "[PASS] Actual task delta: $($actualTaskChanges.Count) file(s)"
        Assert-ChangedScope $script:RepoRoot $packet $actualTaskChanges
        Assert-ExactTaskChanges $script:RepoRoot $packet.required_baseline_commit $declaredChanges $actualRecords
        Write-Output "[PASS] Exact declared/actual create/modify/delete sets match."
        $declaredGateCount = @($packet.verification | Where-Object { $_.required -eq $true }).Count
        $requiredGateCount = @((@($profiles | Where-Object name -eq $packet.context_profile)[0]).mandatory_verification_gates).Count
        Write-Output "[PASS] Mandatory profile gates declared: $requiredGateCount/$requiredGateCount"
        Write-Output "[PASS] Task packet validated: $($packet.task_id)"
        if ($TaskStateOutput) {
            $statePath = Assert-PathOutsideRoot $TaskStateOutput $script:RepoRoot "TaskStateOutput"
            [void](Write-TaskStateSnapshot $statePath $packet $head $actualRecords)
            Write-Output "[PASS] Task state written: $statePath"
        }
    }
    else {
        Write-Output "[PASS] Agent context verification passed."
    }
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
finally {
    Pop-Location
}
