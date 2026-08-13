[CmdletBinding()]
param(
    [string]$TaskPacket,
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$script:RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

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

function Has-Property($Object, [string]$Name) {
    return $null -ne $Object.PSObject.Properties[$Name]
}

function Require-Property($Object, [string]$Name, [string]$Context) {
    if (-not (Has-Property $Object $Name)) { Fail "$Context is missing required field '$Name'" }
    return $Object.PSObject.Properties[$Name].Value
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
        ".agent/state/source-lock.json",
        "scripts/verify-agent-context.ps1"
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
    if ((Require-Property $manifest "schema_version" "manifest") -ne "1.0.0") { Fail "manifest schema_version must be 1.0.0" }
    if ((Require-Property $manifest "context_release" "manifest") -ne "agent-context-1.0.0") { Fail "manifest context_release must be agent-context-1.0.0" }
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
    $expected = @("Cargo.toml", "docs/FOUNDATION_DECISIONS.md", "docs/HOSTED_PARSER.md", "docs/RISK_REGISTER.md", "docs/SECURITY_AND_OPERATIONS.md", "nutrition_backend_blueprint_v1.0/00_README.md", "nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md", "nutrition_backend_blueprint_v1.0/13_IMPLEMENTATION_CHECKLIST.md")
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
}

function Assert-Impacts($Packet, [string[]]$IntendedPaths) {
    $impacts = Require-Property $Packet "impacts" "task packet"
    foreach ($name in @("runtime_behavior", "domain_behavior", "api", "database", "dependencies", "behavior_versions")) {
        $value = Require-Property $impacts $name "task packet impacts"
        if ($value -notin @("none", "specified_change")) { Fail "invalid impact value for $name" }
    }
    if (@($impacts.PSObject.Properties | Where-Object { $_.Value -eq "specified_change" }).Count -gt 0 -and -not (Has-Property $Packet "impact_spec")) {
        Fail "specified_change impact requires impact_spec"
    }
    $normalized = @($IntendedPaths | ForEach-Object { Normalize-RepoPath $_ })
    if ($impacts.dependencies -eq "none" -and @($normalized | Where-Object { $_ -eq "Cargo.toml" -or $_ -eq "Cargo.lock" }).Count -gt 0) { Fail "dependency files changed while dependencies impact is none" }
    if ($impacts.database -eq "none" -and @($normalized | Where-Object { Test-GlobMatch $_ "migrations/**" }).Count -gt 0) { Fail "migration changed while database impact is none" }
    if ($impacts.behavior_versions -eq "none" -and @($normalized | Where-Object { $_ -match '(?i)(behavior.?version|version.?vector|parser.?schema.?version|calculation.?engine.?version|docs/releases/)' }).Count -gt 0) { Fail "behavior-version path changed while behavior_versions impact is none" }
}

function Assert-TaskPacketObject($Packet, [string]$Root, $Profiles) {
    foreach ($name in @("schema_version", "task_id", "objective", "decision_authority", "executor_role", "context_profile", "required_baseline_commit", "allowed_paths", "forbidden_paths", "create_files", "modify_files", "implementation_sequence", "decision_points", "impacts", "acceptance_criteria", "verification", "escalation_conditions", "completion_report_required")) {
        [void](Require-Property $Packet $name "task packet")
    }
    if ($Packet.schema_version -ne "1.0.0" -or $Packet.decision_authority -ne "architect" -or $Packet.executor_role -ne "implementation_only" -or $Packet.completion_report_required -ne $true) { Fail "task packet identity fields are invalid" }
    if ([string]::IsNullOrWhiteSpace($Packet.task_id) -or [string]::IsNullOrWhiteSpace($Packet.objective)) { Fail "task packet task_id/objective must be non-empty" }
    if (@($Packet.decision_points).Count -ne 0) { Fail "task packet decision_points must be empty" }
    $profile = @($Profiles | Where-Object name -eq $Packet.context_profile)
    if ($profile.Count -ne 1) { Fail "unknown or ambiguous context profile: $($Packet.context_profile)" }
    $allowed = @($Packet.allowed_paths | ForEach-Object { Normalize-RepoPath $_ })
    $forbidden = @($Packet.forbidden_paths | ForEach-Object { Normalize-RepoPath $_ })
    if ($allowed.Count -eq 0) { Fail "task packet allowed_paths must be non-empty" }
    if (@($allowed | Where-Object { $_ -in $forbidden }).Count -gt 0) { Fail "task packet allowed and forbidden paths overlap" }
    $intended = @($Packet.create_files | ForEach-Object { Normalize-RepoPath $_ })
    foreach ($modification in @($Packet.modify_files)) {
        [void](Require-Property $modification "path" "task packet modify_files item")
        [void](Require-Property $modification "changes" "task packet modify_files item")
        $intended += Normalize-RepoPath $modification.path
    }
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
    return $intended
}

function Get-GitChangedFiles([string]$Root) {
    $output = @(& git -C $Root diff --name-only)
    if ($LASTEXITCODE -ne 0) { Fail "git diff --name-only failed in task mode" }
    return @($output | ForEach-Object { Normalize-RepoPath $_ } | Where-Object { $_ -ne "" })
}

function Assert-ChangedScope([string]$Root, $Packet) {
    $changed = @(Get-GitChangedFiles $Root)
    $allowed = @($Packet.allowed_paths | ForEach-Object { Normalize-RepoPath $_ })
    $forbidden = @($Packet.forbidden_paths | ForEach-Object { Normalize-RepoPath $_ })
    foreach ($path in $changed) {
        if (@($forbidden | Where-Object { Test-GlobMatch $path $_ }).Count -gt 0) { Fail "changed path is forbidden by task packet: $path" }
        if (@($allowed | Where-Object { Test-GlobMatch $path $_ }).Count -eq 0) { Fail "changed path is outside task allowlist: $path" }
    }
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
    Assert-Template $Root
    return $profiles
}

function New-BaseSelfTestPacket {
    return [ordered]@{
        schema_version = "1.0.0"
        task_id = "ACL-SELFTEST"
        objective = "Exercise ACL verifier"
        decision_authority = "architect"
        executor_role = "implementation_only"
        context_profile = "agent-context-maintenance"
        required_baseline_commit = "da04e773a214e8f8232db149d1f35f3f0bd61ce1"
        allowed_paths = @(".agent/README.md")
        forbidden_paths = @("crates/**", "migrations/**", "Cargo.toml", "Cargo.lock")
        create_files = @()
        modify_files = @(@{path = ".agent/README.md"; changes = @("replace exact sentence")})
        implementation_sequence = @("validate", "change", "verify")
        decision_points = @()
        impacts = [ordered]@{runtime_behavior = "none"; domain_behavior = "none"; api = "none"; database = "none"; dependencies = "none"; behavior_versions = "none"}
        acceptance_criteria = @("scope remains ACL-only")
        verification = @(@{gate = "acl-integrity"; command = ".\\scripts\\verify-agent-context.ps1"; required = $true})
        escalation_conditions = @("outside allowlist")
        completion_report_required = $true
    }
}

function Invoke-SelfTest([string]$Root) {
    $casesDocument = Load-Json (Get-RepoPath $Root ".agent/evals/context-layer-cases.json")
    $cases = @((Require-Property $casesDocument "cases" "self-test cases"))
    if ($cases.Count -lt 12) { Fail "self-test requires at least twelve cases" }
    $tempRoot = Join-Path ([IO.Path]::GetTempPath()) ("agent-context-selftest-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $tempRoot | Out-Null
    try {
        Copy-Item -Recurse -Force -LiteralPath (Get-RepoPath $Root ".agent") -Destination (Join-Path $tempRoot ".agent")
        $lockSources = @("Cargo.toml", "docs/FOUNDATION_DECISIONS.md", "docs/HOSTED_PARSER.md", "docs/RISK_REGISTER.md", "docs/SECURITY_AND_OPERATIONS.md", "nutrition_backend_blueprint_v1.0/00_README.md", "nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md", "nutrition_backend_blueprint_v1.0/13_IMPLEMENTATION_CHECKLIST.md")
        foreach ($source in $lockSources) {
            $destination = Get-RepoPath $tempRoot $source
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
            Copy-Item -Force -LiteralPath (Get-RepoPath $Root $source) -Destination $destination
        }
        $profiles = Assert-Profiles $tempRoot
        foreach ($case in $cases) {
            $packet = New-BaseSelfTestPacket
            $name = $case.name
            switch ($name) {
                "missing_context_profile" { $packet.context_profile = "" }
                "unknown_context_profile" { $packet.context_profile = "missing-profile" }
                "non_empty_decision_points" { $packet.decision_points = @("choose") }
                "allowed_and_forbidden_overlap" { $packet.forbidden_paths += ".agent/README.md" }
                "dependency_change_declared_none" { $packet.modify_files = @(@{path = "Cargo.toml"; changes = @("change dependency")}) }
                "migration_change_declared_none" { $packet.modify_files = @(@{path = "migrations/0002.sql"; changes = @("change schema")}); $packet.impacts.database = "none" }
                "changed_file_outside_allowlist" { $packet.modify_files = @(@{path = "crates/domain/src/lib.rs"; changes = @("runtime change")}) }
                "forbidden_runtime_file_for_acl_task" { $packet.allowed_paths = @(".agent/**"); $packet.modify_files = @(@{path = "crates/domain/src/lib.rs"; changes = @("runtime change")}) }
                "stale_source_hash" { Add-Content -LiteralPath (Get-RepoPath $tempRoot "Cargo.toml") -Value "# stale self-test fixture" }
                "oversized_agents_md_fixture" { Set-Content -LiteralPath (Get-RepoPath $tempRoot "AGENTS.md") -Value ("x" * 4097) }
                "profile_references_missing_file" { Remove-Item -LiteralPath (Get-RepoPath $tempRoot ".agent/contexts/verification.md") }
            }
            $taskPath = Get-RepoPath $tempRoot ("case-" + $name + ".json")
            $packet | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $taskPath -Encoding utf8
            $observedPass = $false
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
            }
            $expectedPass = $case.expected -eq "pass"
            if ($observedPass -ne $expectedPass) { Fail "self-test case '$name' expected $($case.expected) but observed $([string]$observedPass)" }
            Write-Output "[PASS] Self-test: $name"
        }
    }
    finally {
        if (Test-Path -LiteralPath $tempRoot) { Remove-Item -Recurse -Force -LiteralPath $tempRoot }
    }
    Write-Output "[PASS] All $($cases.Count) ACL self-test cases passed."
}

Push-Location $script:RepoRoot
try {
    if ($SelfTest -and $TaskPacket) { Fail "-SelfTest and -TaskPacket cannot be used together" }
    if ($SelfTest) {
        Invoke-SelfTest $script:RepoRoot
        exit 0
    }
    $profiles = Assert-Integrity $script:RepoRoot
    if ($TaskPacket) {
        $packetPath = if ([IO.Path]::IsPathRooted($TaskPacket)) { $TaskPacket } else { Get-RepoPath $script:RepoRoot $TaskPacket }
        $packet = Load-Json $packetPath
        [void](Assert-TaskPacketObject $packet $script:RepoRoot $profiles)
        Assert-ChangedScope $script:RepoRoot $packet
        Write-Output "[PASS] Task packet validated: $($packet.task_id)"
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
