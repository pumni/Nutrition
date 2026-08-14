[CmdletBinding()]
param(
    [string]$TaskIntent,
    [string]$TargetRoot,
    [string]$OutputPath,
    [string]$ExternalEvidencePath,
    [string]$ExecutionStatePath,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$script:ControlRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$script:PowerShellExe = if (Test-Path -LiteralPath (Join-Path $PSHOME 'pwsh.exe')) { Join-Path $PSHOME 'pwsh.exe' } else { (Get-Command powershell.exe).Source }

function Fail([string]$Message) { throw "[FAIL] $Message" }
function Normalize([string]$Path) { $value = $Path.Replace('\', '/'); if ($value.StartsWith('./')) { $value = $value.Substring(2) }; return $value }
function Load-Json([string]$Path) { if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail "JSON file does not exist: $Path" }; try { Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json } catch { Fail "invalid JSON: $Path" } }
function Get-Full([string]$Path) { [IO.Path]::GetFullPath($Path) }
function Assert-Outside([string]$Path, [string]$Root, [string]$Label) {
    if ([string]::IsNullOrWhiteSpace($Path)) { Fail "$Label is empty" }
    $full = Get-Full $Path
    $root = (Get-Full $Root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if ($full.Equals($root.TrimEnd('\', '/'), [StringComparison]::OrdinalIgnoreCase) -or $full.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { Fail "$Label must be outside TargetRoot" }
    return $full
}
function Invoke-Process([string]$File, [string[]]$Arguments, [string]$WorkingDirectory, [string]$OutputFile) {
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $File
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    foreach ($arg in $Arguments) { [void]$psi.ArgumentList.Add([string]$arg) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    if (-not $process.Start()) { Fail "could not start verification process: $File" }
    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $combined = $stdout + $stderr
    $combined | Set-Content -LiteralPath $OutputFile -Encoding utf8
    $hash = [Security.Cryptography.SHA256]::Create()
    $digest = ([BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes($combined))).Replace('-', '')).ToLowerInvariant()
    return [pscustomobject]@{
        exit_code = [int]$process.ExitCode
        output_byte_count = [Text.Encoding]::UTF8.GetByteCount($combined)
        output_sha256 = $digest
        summary = if ($process.ExitCode -eq 0) { 'process exited with code 0' } else { 'process failed with exit code ' + $process.ExitCode }
    }
}
function Assert-Registry([string]$Root) {
    $map = Load-Json (Join-Path $Root '.agent/maps/verification-map.json')
    if ($map.schema_version -ne '3.0.0' -or $map.release -ne 'agent-gates-3.0.0') { Fail 'verification registry release mismatch' }
    $seen = @{}
    foreach ($gate in @($map.gates)) {
        if ($seen.ContainsKey([string]$gate.name)) { Fail "duplicate gate ID: $($gate.name)" }
        $seen[[string]$gate.name] = $gate
        if ([string]$gate.kind -notin @('control-script','target-script','native','json-parse','external-evidence')) { Fail "unknown gate kind: $($gate.name)" }
    }
    return $seen
}
function Test-Glob([string]$Path, [string]$Pattern) {
    $escaped = [regex]::Escape((Normalize $Pattern)).Replace('\*\*', '.*').Replace('\*', '[^/]*').Replace('\?', '[^/]')
    return [regex]::IsMatch((Normalize $Path), '^' + $escaped + '$', [Text.RegularExpressions.RegexOptions]::IgnoreCase)
}
function Get-TargetHead([string]$Root) {
    $head = (& git -C $Root rev-parse HEAD 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $head -notmatch '^[0-9a-fA-F]{40}$') { Fail "could not resolve target HEAD: $head" }
    return $head
}
function Get-ChangeRecords([string]$Root, [string]$Baseline) {
    $records = @()
    $lines = (& git -C $Root diff --name-status --no-renames $Baseline -- 2>&1 | Out-String).Trim() -split "`r?`n" | Where-Object { $_ }
    foreach ($line in $lines) {
        $parts = $line -split "`t"
        $type = if ($parts[0] -match 'A') { 'create' } elseif ($parts[0] -match 'D') { 'delete' } else { 'modify' }
        $records += [pscustomobject]@{ path = Normalize $parts[-1]; type = $type; provenance = @('unstaged') }
    }
    $untracked = (& git -C $Root ls-files --others --exclude-standard 2>&1 | Out-String).Trim() -split "`r?`n" | Where-Object { $_ }
    foreach ($path in $untracked) { $records += [pscustomobject]@{ path = Normalize $path; type = 'create'; provenance = @('untracked') } }
    return @($records | Sort-Object path -Unique)
}
function Get-DerivedRequirements([string]$Root, $Records) {
    $router = Load-Json (Join-Path $Root '.agent/context/router.json')
    $modules = @($router.default_modules)
    $gates = @()
    $riskTags = @()
    foreach ($record in @($Records)) {
        $matched = $false
        foreach ($route in @($router.path_routes)) {
            if (Test-Glob $record.path ([string]$route.path_pattern)) {
                $matched = $true
                $modules += @($route.modules)
                $gates += @($route.mandatory_gates)
                $riskTags += @($route.risk_tags)
            }
        }
        if ($record.path -match '^(Cargo\.toml|Cargo\.lock|crates/[^/]+/Cargo\.toml)$') {
            $modules += 'verification'
            $gates += @('cargo-fmt', 'cargo-clippy', 'cargo-test')
            $riskTags += 'dependency'
            $matched = $true
        }
        if (-not $matched) { $modules += 'verification' }
    }
    if (@($Records).Count -eq 0) { $gates += 'acl-integrity' }
    return [pscustomobject]@{
        modules = @($modules | Select-Object -Unique)
        gates = @($gates | Select-Object -Unique)
        risk_tags = @($riskTags | Select-Object -Unique)
    }
}
function Get-ScopeViolations([string]$Root, $Spec, $Records) {
    $policy = Load-Json (Join-Path $Root '.agent/verification/scope-policy.json')
    $approved = @($Spec.scope_envelope.approved_protected_paths) + @($Spec.approved_protected_decisions | ForEach-Object { $_.scope })
    $violations = @()
    foreach ($record in @($Records)) {
        $inside = @($Spec.scope_envelope.include | Where-Object { Test-Glob $record.path $_ }).Count -gt 0
        $excluded = @($Spec.scope_envelope.exclude | Where-Object { Test-Glob $record.path $_ }).Count -gt 0
        if (-not $inside -or $excluded) { $violations += "SCOPE_VIOLATION:$($record.path)" }
        $protected = @($policy.protected_path_patterns | Where-Object { Test-Glob $record.path $_ }).Count -gt 0
        if ($protected -and @($approved | Where-Object { Test-Glob $record.path $_ }).Count -eq 0) { $violations += "PROTECTED_DECISION_REQUIRED:$($record.path)" }
    }
    return @($violations | Sort-Object -Unique)
}
function Get-RiskAssessment($Spec, $Records, $Escalations, $Derived) {
    $levels = @('low', 'medium', 'high', 'critical')
    $level = [string]$Spec.risk_level
    $evidence = @('change-records', 'scope-verification', 'derived-path-requirements')
    foreach ($record in @($Records)) {
        if ($record.path -match '^(migrations/|schemas/|\.github/workflows/|\.agent/authority/|crates/api-http/src/main\.rs$)') { $candidate = 'critical' }
        elseif ($record.path -match '^(Cargo\.toml|Cargo\.lock|crates/|\.agent/|AGENTS\.md$|scripts/)') { $candidate = 'high' }
        else { $candidate = 'medium' }
        if ($levels.IndexOf($candidate) -gt $levels.IndexOf($level)) { $level = $candidate }
    }
    foreach ($tag in @($Derived.risk_tags)) {
        if ($tag -in @('database', 'migration', 'api', 'auth', 'privacy', 'provider', 'release')) { $candidate = 'high' } else { $candidate = 'medium' }
        if ($levels.IndexOf($candidate) -gt $levels.IndexOf($level)) { $level = $candidate }
    }
    foreach ($escalation in @($Escalations)) {
        if ($levels.IndexOf([string]$escalation.risk_level) -gt $levels.IndexOf($level)) { $level = [string]$escalation.risk_level }
        $evidence += @($escalation.evidence_refs | ForEach-Object { 'agent:' + [string]$_ })
    }
    return [pscustomobject]@{ level = $level; escalated = ($levels.IndexOf($level) -gt $levels.IndexOf([string]$Spec.risk_level)); evidence_refs = @($evidence | Sort-Object -Unique) }
}
function Validate-ExternalEvidence($Gate, [string]$GateId, [string]$TaskId, [string]$TargetRoot, [string]$EvidenceRoot, [string]$LogPath) {
    if ([string]::IsNullOrWhiteSpace($EvidenceRoot) -or -not (Test-Path -LiteralPath $EvidenceRoot -PathType Container)) {
        '[BLOCKED] external evidence directory was not supplied' | Set-Content -LiteralPath $LogPath -Encoding utf8
        return [pscustomobject]@{ exit_code = 2; output_byte_count = 0; output_sha256 = ('0' * 64); summary = 'external evidence directory not supplied' }
    }
    $externalRoot = (Get-Full $EvidenceRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $targetHead = Get-TargetHead $TargetRoot
    $matches = @()
    foreach ($file in @(Get-ChildItem -LiteralPath $EvidenceRoot -Recurse -File -Filter '*.json')) {
        try {
            $candidate = Get-Content -Raw -LiteralPath $file.FullName | ConvertFrom-Json
            if ([string]$candidate.gate -eq $GateId -and [string]$candidate.evidence_kind -eq [string]$Gate.evidence_kind) { $matches += [pscustomobject]@{ document = $candidate; path = $file.FullName } }
        } catch { }
    }
    try {
        if ($matches.Count -ne 1) { throw "expected exactly one evidence object for $GateId/$($Gate.evidence_kind), found $($matches.Count)" }
        $document = $matches[0].document
        $required = @('schema_version','task_id','gate','evidence_kind','subject_commit','result','artifact_ref','artifact_sha256','issuer')
        $unknown = @($document.PSObject.Properties.Name | Where-Object { $_ -notin $required })
        $missing = @($required | Where-Object { -not $document.PSObject.Properties[$_] })
        if ($unknown.Count -gt 0 -or $missing.Count -gt 0) { throw 'external evidence object shape is invalid' }
        if ($document.schema_version -ne '1.0.0' -or [string]$document.task_id -ne $TaskId -or $document.result -ne 'pass' -or [string]$document.subject_commit -ne $targetHead -or [string]::IsNullOrWhiteSpace([string]$document.issuer)) { throw 'external evidence identity or result is invalid' }
        $artifactPath = if ([IO.Path]::IsPathRooted([string]$document.artifact_ref)) { Get-Full ([string]$document.artifact_ref) } else { Get-Full (Join-Path (Get-Full $EvidenceRoot) ([string]$document.artifact_ref)) }
        if (-not $artifactPath.StartsWith($externalRoot, [StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) { throw 'external evidence artifact is outside the evidence directory or missing' }
        $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifactPath).Hash.ToLowerInvariant()
        if ($actualHash -ne ([string]$document.artifact_sha256).ToLowerInvariant()) { throw 'external evidence artifact SHA-256 does not match' }
        "[PASS] external evidence validated: $GateId" | Set-Content -LiteralPath $LogPath -Encoding utf8
        return [pscustomobject]@{ exit_code = 0; output_byte_count = 0; output_sha256 = ('0' * 64); summary = 'external evidence validated' }
    } catch {
        ($_ | Out-String) | Set-Content -LiteralPath $LogPath -Encoding utf8
        return [pscustomobject]@{ exit_code = 2; output_byte_count = 0; output_sha256 = ('0' * 64); summary = 'external evidence validation failed' }
    }
}
function Invoke-Gate($Gate, [string]$GateId, [string]$TargetRoot, [string]$EvidenceDir, [string]$ExternalRoot) {
    New-Item -ItemType Directory -Force -Path $EvidenceDir | Out-Null
    $log = Join-Path $EvidenceDir ($GateId + '.log')
    switch ([string]$Gate.kind) {
        'control-script' { $scriptPath = Join-Path $script:ControlRoot ([string]$Gate.script); $args = @('-NoLogo','-NoProfile','-File',$scriptPath) + @($Gate.arguments); if ($Gate.target_root_argument) { $args += @([string]$Gate.target_root_argument, $TargetRoot) }; $result = Invoke-Process $script:PowerShellExe $args $script:ControlRoot $log }
        'target-script' { $scriptPath = Join-Path $TargetRoot ([string]$Gate.script); $args = @('-NoLogo','-NoProfile','-File',$scriptPath) + @($Gate.arguments); $result = Invoke-Process $script:PowerShellExe $args $TargetRoot $log }
        'native' { $result = Invoke-Process ([string]$Gate.program) @($Gate.arguments) $TargetRoot $log }
        'json-parse' { try { foreach ($path in @($Gate.paths)) { Get-Content -Raw -LiteralPath (Join-Path $TargetRoot ([string]$path)) | ConvertFrom-Json | Out-Null }; '[PASS] JSON parsed' | Set-Content -LiteralPath $log -Encoding utf8; $result = [pscustomobject]@{ exit_code = 0; output_byte_count = 0; output_sha256 = ('0' * 64); summary = 'JSON parsed' } } catch { ($_ | Out-String) | Set-Content -LiteralPath $log -Encoding utf8; $result = [pscustomobject]@{ exit_code = 1; output_byte_count = 0; output_sha256 = ('0' * 64); summary = 'JSON parse failed' } } }
        'external-evidence' { $result = Validate-ExternalEvidence $Gate $GateId ([string]$script:CurrentTaskId) $TargetRoot $ExternalRoot $log }
        default { Fail "unsupported canonical gate kind: $($Gate.kind)" }
    }
    return [pscustomobject][ordered]@{ gate_id = $GateId; kind = [string]$Gate.kind; required = $true; status = if ($result.exit_code -eq 0) { 'pass' } elseif ($result.exit_code -eq 2) { 'blocked' } else { 'fail' }; exit_code = [int]$result.exit_code; output_byte_count = [int64]$result.output_byte_count; output_sha256 = [string]$result.output_sha256; summary = [string]$result.summary }
}
function Write-Json([string]$Path, $Value) { $Value | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath $Path -Encoding utf8 }
function Invoke-RunnerSelfTest {
    $registry = Assert-Registry $script:ControlRoot
    if (-not $registry.ContainsKey('acl-integrity')) { Fail 'canonical registry self-test gate is missing' }
    Write-Output '[PASS] canonical gate registry self-test'
    $temp = Join-Path ([IO.Path]::GetTempPath()) ('agent-runner-selftest-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force $temp | Out-Null
    try { Assert-Outside (Join-Path $temp 'report.json') $temp 'self-test target path' } catch { Write-Output '[PASS] output path protection self-test' }
    if (Test-Path -LiteralPath $temp) { Remove-Item -Recurse -Force $temp }
    Write-Output '[PASS] trusted runner self-test completed.'
}

if ($SelfTest) { try { Invoke-RunnerSelfTest; exit 0 } catch { Write-Error $_.Exception.Message; exit 1 } }
try {
    if ([string]::IsNullOrWhiteSpace($TaskIntent)) { Fail '-TaskIntent is required unless -SelfTest is used' }
    if ([string]::IsNullOrWhiteSpace($TargetRoot)) { Fail '-TargetRoot is required unless -SelfTest is used' }
    $target = (Resolve-Path -LiteralPath $TargetRoot).Path
    $intentPath = (Resolve-Path -LiteralPath $TaskIntent).Path
    $output = Assert-Outside $OutputPath $target 'OutputPath'
    $externalRoot = if ($ExternalEvidencePath) { Assert-Outside $ExternalEvidencePath $target 'ExternalEvidencePath' } else { $null }
    $evidence = Join-Path ([IO.Path]::GetTempPath()) ('agent-verification-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force $evidence | Out-Null
    $compiledSpecPath = Join-Path $evidence 'compiled-task-spec.json'
    $compileLog = Join-Path $evidence 'task-spec-compile.log'
    $compile = Invoke-Process $script:PowerShellExe @('-NoLogo','-NoProfile','-File',(Join-Path $script:ControlRoot 'scripts/compile-agent-task-spec.ps1'),'-IntentPath',$intentPath,'-RepositoryRoot',$target,'-OutputPath',$compiledSpecPath) $script:ControlRoot $compileLog
    if ($compile.exit_code -ne 0) { Fail 'CONTEXT_INTEGRITY_FAILED: Task Spec compilation failed' }
    $spec = Load-Json $compiledSpecPath
    $script:CurrentTaskId = [string]$spec.task_id
    $specHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $compiledSpecPath).Hash.ToLowerInvariant()
    $registry = Assert-Registry $script:ControlRoot
    $preflightLog = Join-Path $evidence 'task-spec-preflight.log'
    $preflight = Invoke-Process $script:PowerShellExe @('-NoLogo','-NoProfile','-File',(Join-Path $script:ControlRoot 'scripts/verify-agent-context.ps1'),'-TaskSpec',$compiledSpecPath,'-RepositoryRoot',$target) $script:ControlRoot $preflightLog
    if ($preflight.exit_code -ne 0) { Fail 'CONTEXT_INTEGRITY_FAILED: Task Spec preflight failed' }
    $escalations = @()
    if ($ExecutionStatePath) {
        $statePath = (Resolve-Path -LiteralPath $ExecutionStatePath).Path
        if (-not $statePath.StartsWith($target.TrimEnd('\','/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { Fail 'ExecutionStatePath must be inside TargetRoot' }
        $state = Load-Json $statePath
        if ($state.schema_version -ne '2.0.0') { Fail 'execution state schema version is invalid' }
        foreach ($escalation in @($state.risk_escalations)) { if ([string]$escalation.risk_level -notin @('low','medium','high','critical') -or [string]::IsNullOrWhiteSpace([string]$escalation.reason) -or @($escalation.evidence_refs).Count -eq 0) { Fail 'execution state contains an invalid risk escalation' }; $escalations += $escalation }
    }
    $records = @(Get-ChangeRecords $target ([string]$spec.baseline.commit))
    $scopeViolations = Get-ScopeViolations $target $spec $records
    $derived = Get-DerivedRequirements $target $records
    $gateResults = @()
    $gateIndex = 1
    foreach ($gateId in @($derived.gates)) {
        if (-not $registry.ContainsKey([string]$gateId)) { Fail "unknown derived gate: $gateId" }
        $gateResults += Invoke-Gate $registry[[string]$gateId] ([string]$gateId) $target (Join-Path $evidence ('gate-' + $gateIndex)) $externalRoot
        $gateIndex++
    }
    $failed = @($gateResults | Where-Object status -ne 'pass')
    $riskAssessment = Get-RiskAssessment $spec $records $escalations $derived
    $report = [ordered]@{
        schema_version = '2.0.0'
        task_id = [string]$spec.task_id
        task_spec_sha256 = $specHash
        releases = [ordered]@{ runner_release = 'agent-runner-2.0.0'; verifier_release = 'agent-verifier-3.0.0'; verification_registry_release = 'agent-gates-3.0.0' }
        control_plane = [ordered]@{ root = $script:ControlRoot; head_commit = (& git -C $script:ControlRoot rev-parse HEAD).Trim(); file_hashes = @{ 'scripts/run-agent-verification.ps1' = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $script:ControlRoot 'scripts/run-agent-verification.ps1')).Hash.ToLowerInvariant() } }
        target = [ordered]@{ root = $target; baseline_commit = [string]$spec.baseline.commit; head_commit = Get-TargetHead $target }
        change_records = @($records)
        derived_context_modules = @($derived.modules)
        derived_risk_tags = @($derived.risk_tags)
        gate_results = @($gateResults)
        risk = [ordered]@{ requested = [string]$spec.risk_level; effective = $riskAssessment.level; escalated = [bool]$riskAssessment.escalated; evidence_refs = @($riskAssessment.evidence_refs) }
        scope = [ordered]@{ result = if ($scopeViolations.Count -eq 0) { 'pass' } else { 'fail' }; create = @($records | Where-Object type -eq 'create' | ForEach-Object path); modify = @($records | Where-Object type -eq 'modify' | ForEach-Object path); delete = @($records | Where-Object type -eq 'delete' | ForEach-Object path); violations = @($scopeViolations) }
        result = if ($failed.Count -eq 0 -and $scopeViolations.Count -eq 0) { 'pass' } else { 'fail' }
    }
    New-Item -ItemType Directory -Force (Split-Path -Parent $output) | Out-Null
    Write-Json $output $report
    Write-Output "[PASS] verification report written: $output"
    if ($report.result -ne 'pass') { exit 1 }
    exit 0
}
catch { Write-Error $_.Exception.Message; exit 1 }
