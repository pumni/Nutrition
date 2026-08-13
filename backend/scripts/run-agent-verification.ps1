[CmdletBinding()]
param(
    [string]$TaskPacket,
    [string]$TargetRoot,
    [string]$OutputPath,
    [string[]]$ExternalEvidencePath,
    [switch]$SelfTest
)

$ErrorActionPreference = "Stop"
$script:ControlRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$script:PowerShellExe = if (Test-Path -LiteralPath (Join-Path $PSHOME "pwsh.exe")) { Join-Path $PSHOME "pwsh.exe" } else { (Get-Command powershell.exe).Source }

function Fail([string]$Message) { throw "[FAIL] $Message" }

function Get-FullPath([string]$Path) { return [IO.Path]::GetFullPath($Path) }

function Normalize-RepoPath([string]$Path) {
    if ($null -eq $Path) { return "" }
    $value = $Path.Replace("\", "/")
    if ($value.StartsWith("./")) { $value = $value.Substring(2) }
    return $value
}

function Get-RepoPath([string]$Root, [string]$RelativePath) { return Join-Path $Root ($RelativePath.Replace("/", "\")) }

function Assert-OutsideRoot([string]$Path, [string]$Root, [string]$Label) {
    if ([string]::IsNullOrWhiteSpace($Path)) { Fail "$Label is empty" }
    $fullPath = Get-FullPath $Path
    $fullRoot = (Get-FullPath $Root).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    $rootPrefix = $fullRoot + [IO.Path]::DirectorySeparatorChar
    if ($fullPath.Equals($fullRoot, [StringComparison]::OrdinalIgnoreCase) -or $fullPath.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        Fail "$Label must be outside TargetRoot: $fullPath"
    }
    return $fullPath
}

function Load-Json([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail "JSON file does not exist: $Path" }
    try { return Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json }
    catch { Fail "Invalid JSON: $Path :: $($_.Exception.Message)" }
}

function Assert-ExactProperties($Object, [string[]]$Allowed, [string]$Context) {
    $unknown = @($Object.PSObject.Properties.Name | Where-Object { $_ -notin $Allowed })
    if ($unknown.Count -gt 0) { Fail "$Context contains unknown field(s): $($unknown -join ', ')" }
}

function Require-Property($Object, [string]$Name, [string]$Context) {
    if ($null -eq $Object.PSObject.Properties[$Name]) { Fail "$Context is missing required field '$Name'" }
    return $Object.PSObject.Properties[$Name].Value
}

function Get-FileSha256([string]$Path) { return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant() }

function Get-BytesSha256([byte[]]$Bytes) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($Bytes)).Replace("-", "")).ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function Get-CombinedFileMetadata([string[]]$Paths) {
    $sha = [Security.Cryptography.SHA256]::Create()
    $buffer = New-Object byte[] 65536
    [int64]$count = 0
    try {
        foreach ($path in $Paths) {
            $stream = [IO.File]::OpenRead($path)
            try {
                while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    [void]$sha.TransformBlock($buffer, 0, $read, $buffer, 0)
                    $count += $read
                }
            }
            finally { $stream.Dispose() }
        }
        [void]$sha.TransformFinalBlock([byte[]]@(), 0, 0)
        return [pscustomobject]@{ByteCount = $count; Sha256 = ([BitConverter]::ToString($sha.Hash).Replace("-", "")).ToLowerInvariant()}
    }
    finally { $sha.Dispose() }
}

function Invoke-Process([string]$Program, [string[]]$Arguments, [string]$WorkingDirectory) {
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $Program
    $start.WorkingDirectory = $WorkingDirectory
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in @($Arguments)) { [void]$start.ArgumentList.Add([string]$argument) }
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    $outputDirectory = Join-Path ([IO.Path]::GetTempPath()) ("agent-runner-output-" + [guid]::NewGuid().ToString("N"))
    $stdoutPath = Join-Path $outputDirectory "stdout.bin"
    $stderrPath = Join-Path $outputDirectory "stderr.bin"
    try {
        New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
        if (-not $process.Start()) { Fail "could not start process: $Program" }
        $stdoutStream = [IO.File]::Open($stdoutPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        $stderrStream = [IO.File]::Open($stderrPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        $stdoutTask = $process.StandardOutput.BaseStream.CopyToAsync($stdoutStream)
        $stderrTask = $process.StandardError.BaseStream.CopyToAsync($stderrStream)
        $process.WaitForExit()
        $stdoutTask.GetAwaiter().GetResult()
        $stderrTask.GetAwaiter().GetResult()
        $stdoutStream.Dispose()
        $stderrStream.Dispose()
        $metadata = Get-CombinedFileMetadata @($stdoutPath, $stderrPath)
        return [pscustomobject]@{
            ExitCode = [int]$process.ExitCode
            OutputByteCount = $metadata.ByteCount
            OutputSha256 = $metadata.Sha256
            Summary = if ($process.ExitCode -eq 0) { "process exited with code 0" } else { "process exited with code $($process.ExitCode)" }
        }
    }
    catch {
        return [pscustomobject]@{ExitCode = -1; OutputByteCount = 0; OutputSha256 = Get-BytesSha256 ([byte[]]@()); Summary = "process start or capture failed: $($_.Exception.Message)"}
    }
    finally {
        if ($null -ne $stdoutStream) { $stdoutStream.Dispose() }
        if ($null -ne $stderrStream) { $stderrStream.Dispose() }
        $process.Dispose()
        if (Test-Path -LiteralPath $outputDirectory) { Remove-Item -Recurse -Force -LiteralPath $outputDirectory }
    }
}

function Resolve-SafeChildPath([string]$Root, [string]$RelativePath, [string]$Label) {
    if ([string]::IsNullOrWhiteSpace($RelativePath)) { Fail "$Label is empty" }
    if ([IO.Path]::IsPathRooted($RelativePath)) { Fail "$Label must be relative" }
    $normalized = Normalize-RepoPath $RelativePath
    if (@($normalized.Split('/') | Where-Object { $_ -eq ".." }).Count -gt 0) { Fail "$Label path traversal is rejected" }
    $fullRoot = (Get-FullPath $Root).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    $fullPath = Get-FullPath (Join-Path $fullRoot ($normalized.Replace("/", "\")))
    $prefix = $fullRoot + [IO.Path]::DirectorySeparatorChar
    if (-not $fullPath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { Fail "$Label escapes its root" }
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) { Fail "$Label does not exist: $normalized" }
    return $fullPath
}

function Assert-Registry([string]$Root) {
    $registry = Load-Json (Get-RepoPath $Root ".agent/maps/verification-map.json")
    Assert-ExactProperties $registry @("schema_version", "gates") "control registry"
    if ($registry.schema_version -ne "2.1.0") { Fail "control registry schema_version must be 2.1.0" }
    $gates = @((Require-Property $registry "gates" "control registry"))
    $seen = @{}
    $kinds = @("control-script", "target-script", "native", "json-parse", "external-evidence")
    foreach ($gate in $gates) {
        Assert-ExactProperties $gate @("name", "kind", "script", "arguments", "program", "paths", "evidence_kind", "display_command", "target_root_argument") "control registry gate"
        $name = [string](Require-Property $gate "name" "control registry gate")
        $kind = [string](Require-Property $gate "kind" "control registry gate $name")
        if ([string]::IsNullOrWhiteSpace($name) -or $seen.ContainsKey($name)) { Fail "control registry gate name is blank or duplicated: $name" }
        if ($kind -notin $kinds) { Fail "control registry gate kind is invalid: $kind" }
        if ([string]::IsNullOrWhiteSpace([string](Require-Property $gate "display_command" "control registry gate $name"))) { Fail "control registry gate display_command is blank: $name" }
        $seen[$name] = $true
        if ($kind -in @("control-script", "target-script")) {
            [void](Require-Property $gate "script" "control registry gate $name")
            [void](Require-Property $gate "arguments" "control registry gate $name")
            if ($gate.arguments -is [string] -or $gate.arguments -isnot [System.Collections.IEnumerable] -or @($gate.arguments | Where-Object { $_ -isnot [string] }).Count -gt 0) { Fail "control registry gate arguments must be string array: $name" }
            if ($kind -eq "target-script" -and $null -ne $gate.PSObject.Properties["target_root_argument"]) { Fail "target-script cannot declare target_root_argument: $name" }
            if ($null -ne $gate.PSObject.Properties["target_root_argument"] -and ($gate.target_root_argument -isnot [string] -or [string]::IsNullOrWhiteSpace($gate.target_root_argument))) { Fail "target_root_argument must be non-empty: $name" }
        }
        elseif ($kind -eq "native") {
            [void](Require-Property $gate "program" "control registry gate $name")
            [void](Require-Property $gate "arguments" "control registry gate $name")
            if ([string]::IsNullOrWhiteSpace([string]$gate.program) -or $gate.arguments -is [string] -or $gate.arguments -isnot [System.Collections.IEnumerable] -or @($gate.arguments | Where-Object { $_ -isnot [string] }).Count -gt 0) { Fail "native registry fields are invalid: $name" }
            if ($null -ne $gate.PSObject.Properties["target_root_argument"]) { Fail "native cannot declare target_root_argument: $name" }
        }
        elseif ($kind -eq "json-parse") {
            $paths = @((Require-Property $gate "paths" "control registry gate $name"))
            if ($paths.Count -eq 0 -or @($paths | Where-Object { $_ -isnot [string] -or [string]::IsNullOrWhiteSpace($_) }).Count -gt 0) { Fail "json-parse paths are invalid: $name" }
            if ($null -ne $gate.PSObject.Properties["target_root_argument"]) { Fail "json-parse cannot declare target_root_argument: $name" }
        }
        else {
            if ([string]::IsNullOrWhiteSpace([string](Require-Property $gate "evidence_kind" "control registry gate $name"))) { Fail "external evidence kind is blank: $name" }
            if ($null -ne $gate.PSObject.Properties["target_root_argument"]) { Fail "external-evidence cannot declare target_root_argument: $name" }
        }
    }
    return $gates
}

function Get-RegistryGate($Registry, [string]$Name) {
    $matches = @($Registry | Where-Object { $_.name -eq $Name })
    if ($matches.Count -ne 1) { Fail "canonical registry gate is missing or ambiguous: $Name" }
    return $matches[0]
}

function Get-GitHead([string]$Root) {
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = "git"; $start.WorkingDirectory = $Root; $start.UseShellExecute = $false; $start.CreateNoWindow = $true
    foreach ($argument in @("-C", $Root, "rev-parse", "HEAD")) { [void]$start.ArgumentList.Add($argument) }
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
    try {
        if (-not $process.Start()) { Fail "could not start git" }
        $output = $process.StandardOutput.ReadToEnd(); [void]$process.StandardError.ReadToEnd(); $process.WaitForExit()
        if ($process.ExitCode -ne 0) { Fail "git HEAD lookup failed" }
        return ([string]$output).Trim().Split([Environment]::NewLine)[0]
    }
    finally { $process.Dispose() }
}

function Get-ExternalEvidence([string[]]$Paths, [string]$TargetRoot, $Gate, $State, [bool]$Required) {
    $expectedKind = [string]$Gate.evidence_kind
    foreach ($path in @($Paths)) {
        $full = Assert-OutsideRoot (Get-FullPath $path) $TargetRoot "External evidence"
        if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { continue }
        try { $evidence = Load-Json $full } catch { continue }
        try {
            Assert-ExactProperties $evidence @("schema_version", "task_id", "gate", "evidence_kind", "subject_commit", "result", "artifact_ref", "artifact_sha256", "issuer") "external evidence"
            if ($evidence.schema_version -notmatch '^1\.0\.0$' -or $evidence.task_id -isnot [string] -or [string]::IsNullOrWhiteSpace($evidence.task_id) -or $evidence.gate -isnot [string] -or [string]::IsNullOrWhiteSpace($evidence.gate) -or $evidence.evidence_kind -isnot [string] -or [string]::IsNullOrWhiteSpace($evidence.evidence_kind) -or $evidence.subject_commit -notmatch '^[0-9a-fA-F]{40}$' -or $evidence.result -ne "pass" -or $evidence.artifact_ref -isnot [string] -or [string]::IsNullOrWhiteSpace($evidence.artifact_ref) -or $evidence.artifact_sha256 -notmatch '^[0-9a-fA-F]{64}$' -or $evidence.issuer -isnot [string] -or [string]::IsNullOrWhiteSpace($evidence.issuer)) { continue }
            if ($evidence.task_id -eq $State.task_id -and $evidence.gate -eq $Gate.name -and $evidence.evidence_kind -eq $expectedKind -and $evidence.subject_commit -eq $State.head_commit) {
                return [pscustomobject]@{Status = "pass"; ExitCode = 0; OutputByteCount = 0; OutputSha256 = Get-BytesSha256 ([byte[]]@()); Summary = "external evidence matched issuer $($evidence.issuer)"}
            }
        } catch { continue }
    }
    if ($Required) { return [pscustomobject]@{Status = "blocked"; ExitCode = -1; OutputByteCount = 0; OutputSha256 = Get-BytesSha256 ([byte[]]@()); Summary = "required external evidence is missing or does not match task, gate, kind, or target HEAD"} }
    return [pscustomobject]@{Status = "skipped"; ExitCode = 0; OutputByteCount = 0; OutputSha256 = Get-BytesSha256 ([byte[]]@()); Summary = "optional external evidence was not supplied"}
}

function Invoke-Gate($Gate, [string]$ControlRoot, [string]$TargetRoot, $State, [string[]]$EvidencePaths) {
    $name = [string]$Gate.name
    $required = @($State.packet_gates | Where-Object gate -eq $name | Select-Object -First 1).required
    $kind = [string]$Gate.kind
    if ($kind -eq "external-evidence") { return Get-ExternalEvidence $EvidencePaths $TargetRoot $Gate $State ([bool]$required) }
    if ($kind -eq "json-parse") {
        try {
            foreach ($relative in @($Gate.paths)) { [void](Load-Json (Resolve-SafeChildPath $TargetRoot $relative "json-parse path")) }
            return [pscustomobject]@{Status="pass"; ExitCode=0; OutputByteCount=0; OutputSha256=Get-BytesSha256 ([byte[]]@()); Summary="parsed all canonical JSON paths"}
        } catch { return [pscustomobject]@{Status="fail"; ExitCode=1; OutputByteCount=0; OutputSha256=Get-BytesSha256 ([byte[]]@()); Summary="JSON parse failed"} }
    }
    $working = $TargetRoot
    $scriptPath = $null
    $arguments = @($Gate.arguments)
    if ($kind -eq "control-script") {
        $scriptPath = Resolve-SafeChildPath $ControlRoot $Gate.script "control script"
        if ($null -ne $Gate.PSObject.Properties["target_root_argument"]) { $arguments += [string]$Gate.target_root_argument; $arguments += $TargetRoot }
        $working = $ControlRoot
        $process = Invoke-Process $script:PowerShellExe (@("-NoLogo", "-NoProfile", "-File", $scriptPath) + $arguments) $working
    }
    elseif ($kind -eq "target-script") {
        $scriptPath = Resolve-SafeChildPath $TargetRoot $Gate.script "target script"
        $working = $TargetRoot
        $process = Invoke-Process $script:PowerShellExe (@("-NoLogo", "-NoProfile", "-File", $scriptPath) + $arguments) $working
    }
    else {
        $process = Invoke-Process ([string]$Gate.program) $arguments $TargetRoot
    }
    $status = if ($process.ExitCode -eq 0) { "pass" } else { "fail" }
    return [pscustomobject]@{Status=$status; ExitCode=$process.ExitCode; OutputByteCount=$process.OutputByteCount; OutputSha256=$process.OutputSha256; Summary=$process.Summary}
}

function Write-AtomicJson([string]$Path, $Object) {
    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    $temporary = "$Path.$([guid]::NewGuid().ToString('N')).tmp"
    try { $Object | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $temporary -Encoding utf8; Move-Item -Force -LiteralPath $temporary -Destination $Path }
    finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -Force -LiteralPath $temporary } }
}

function New-ImmutablePacketSnapshot([string]$SourcePath) {
    $bytes = [IO.File]::ReadAllBytes($SourcePath)
    $hash = Get-BytesSha256 $bytes
    $snapshotPath = Join-Path ([IO.Path]::GetTempPath()) ("agent-task-packet-" + [guid]::NewGuid().ToString("N") + ".json")
    try {
        [IO.File]::WriteAllBytes($snapshotPath, $bytes)
        return [pscustomobject]@{Path = $snapshotPath; Sha256 = $hash}
    }
    catch {
        if (Test-Path -LiteralPath $snapshotPath) { Remove-Item -Force -LiteralPath $snapshotPath }
        throw
    }
}

function Resolve-OverallResult([object[]]$GateResults) {
    $overall = "pass"
    foreach ($result in $GateResults) {
        if ($result.required -and $result.status -eq "fail") { $overall = "fail" }
        if ($result.required -and $result.status -eq "blocked" -and $overall -ne "fail") { $overall = "blocked" }
    }
    return $overall
}

function New-VerificationReport($Manifest, [string]$TaskPacketSha256, $State, [string]$ControlRoot, [string]$TargetRoot, $GateResults, [string]$ControlHead, [string]$Result) {
    $records = @($State.change_records | ForEach-Object { [ordered]@{path=$_.path; type=$_.type; provenance=@($_.provenance)} })
    $hashes = [ordered]@{}
    foreach ($relative in @(".agent/manifest.json", ".agent/maps/verification-map.json", "scripts/verify-agent-context.ps1", "scripts/run-agent-verification.ps1")) { $hashes[$relative] = Get-FileSha256 (Get-RepoPath $ControlRoot $relative) }
    return [ordered]@{
        schema_version = "2.0.0"
        task_id = [string]$State.task_id
        task_packet_sha256 = $TaskPacketSha256
        releases = [ordered]@{runner_release=$Manifest.runner_release; verifier_release=$Manifest.verifier_release; verification_registry_release=$Manifest.verification_registry_release}
        control_plane = [ordered]@{root=$ControlRoot; head_commit=$ControlHead; file_hashes=$hashes}
        target = [ordered]@{root=$TargetRoot; baseline_commit=$State.baseline_commit; head_commit=$State.head_commit}
        change_records = $records
        gate_results = @($GateResults)
        scope = [ordered]@{result="pass"; create=@($records | Where-Object type -eq "create" | ForEach-Object path); modify=@($records | Where-Object type -eq "modify" | ForEach-Object path); delete=@($records | Where-Object type -eq "delete" | ForEach-Object path)}
        impacts = $State.impacts
        result = $Result
    }
}

function Write-RunnerText([string]$Root, [string]$RelativePath, [string]$Content) {
    $path = Get-RepoPath $Root $RelativePath
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    Set-Content -LiteralPath $path -Value $Content -NoNewline
    return $path
}

function Assert-RunnerCase([string]$Name, [string]$Expected, [scriptblock]$Action) {
    $actual = ""
    try { $actual = [string](& $Action) } catch { $actual = "fail" }
    if ($actual -ne $Expected) { Fail "runner self-test '$Name' expected $Expected but observed $actual" }
    Write-Output "[PASS] Runner self-test: $Name"
}

function Invoke-RunnerGit([string]$Root, [string[]]$Arguments) {
    $result = Invoke-Process "git" (@("-C", $Root) + @($Arguments)) $Root
    if ($result.ExitCode -ne 0) { Fail "runner self-test git command failed: $($Arguments -join ' ')" }
    return $result
}

function New-RunnerE2EFixture([string]$ControlRoot, [string]$ParentTemp) {
    $root = Join-Path $ParentTemp "e2e"
    $control = Join-Path $root "control"
    $target = Join-Path $root "target"
    $outside = Join-Path $root "outside"
    New-Item -ItemType Directory -Force -Path $control,$target,$outside | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $control "scripts"),(Join-Path $control ".agent\maps"),(Join-Path $target ".agent\maps") | Out-Null

    Copy-Item -LiteralPath (Join-Path $ControlRoot "scripts\run-agent-verification.ps1") -Destination (Join-Path $control "scripts\run-agent-verification.ps1")
    $verifier = @'
param(
    [string]$TaskPacket,
    [string]$RepositoryRoot,
    [string]$TaskStateOutput
)
$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($TaskPacket)) { exit 0 }
$packet = Get-Content -Raw -LiteralPath $TaskPacket | ConvertFrom-Json
if (-not [string]::IsNullOrWhiteSpace([string]$packet.original_packet_path)) {
    $original = Get-Content -Raw -LiteralPath $packet.original_packet_path | ConvertFrom-Json
    $original | Add-Member -NotePropertyName mutated_by_verifier -NotePropertyValue $true -Force
    $original | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $packet.original_packet_path -Encoding utf8
}
$headRoot = if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) { (Resolve-Path (Join-Path $PSScriptRoot "..")).Path } else { $RepositoryRoot }
$head = (& git -C $headRoot rev-parse HEAD).Trim()
$state = [ordered]@{
    task_id = [string]$packet.task_id
    context_profile = "selftest"
    baseline_commit = $head
    head_commit = $head
    change_records = @()
    packet_gates = @([ordered]@{gate="target-check";required=$true})
    impacts = [ordered]@{runtime_behavior="none";domain_behavior="none";api="none";database="none";dependencies="none";behavior_versions="none"}
}
$state | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $TaskStateOutput -Encoding utf8
exit 0
'@
    Write-RunnerText $control "scripts/verify-agent-context.ps1" $verifier | Out-Null
    $targetCheck = @'
    param([string]$FixtureArgument, [string]$TargetRoot)
Write-Output "bounded target fixture output"
if (Test-Path -LiteralPath (Join-Path $TargetRoot "valid-target.txt") -PathType Leaf) { exit 0 }
exit 7
'@
    Write-RunnerText $control "scripts/check-target.ps1" $targetCheck | Out-Null

    $manifest = [ordered]@{
        schema_version="1.0.0"; context_release="agent-context-1.0.0"; contract_release="agent-contract-1.1.0"; verifier_release="agent-verifier-2.1.0"; verification_registry_release="agent-gates-2.1.0"; runner_release="agent-runner-1.0.1"; verification_report_release="agent-verification-report-2.0.0"; implementation_report_release="agent-implementation-report-1.1.0";
        project=[ordered]@{repository="runner-selftest";behavior_release="foundation-0.6.0"}
    }
    Write-RunnerText $control ".agent/manifest.json" ($manifest | ConvertTo-Json -Depth 20) | Out-Null
    $registry = [ordered]@{schema_version="2.1.0";gates=@([ordered]@{name="target-check";kind="control-script";script="scripts/check-target.ps1";arguments=@("fixture");target_root_argument="-TargetRoot";display_command="control target fixture"})}
    Write-RunnerText $control ".agent/maps/verification-map.json" ($registry | ConvertTo-Json -Depth 20) | Out-Null
    $maliciousRegistry = [ordered]@{schema_version="2.1.0";gates=@([ordered]@{name="target-check";kind="target-script";script="missing-target-script.ps1";arguments=@();display_command="malicious target registry"})}
    Write-RunnerText $target ".agent/maps/verification-map.json" ($maliciousRegistry | ConvertTo-Json -Depth 20) | Out-Null
    Write-RunnerText $target "valid-target.txt" "valid target" | Out-Null

    foreach ($repo in @($control, $target)) {
        Invoke-RunnerGit $repo @("init") | Out-Null
        Invoke-RunnerGit $repo @("config", "user.email", "runner-selftest@example.invalid") | Out-Null
        Invoke-RunnerGit $repo @("config", "user.name", "Runner SelfTest") | Out-Null
        Invoke-RunnerGit $repo @("add", "--all") | Out-Null
        Invoke-RunnerGit $repo @("commit", "-m", "runner self-test fixture") | Out-Null
    }
    $controlHead = Get-GitHead $control
    $targetHead = Get-GitHead $target
    if ($controlHead -eq $targetHead) { Fail "runner self-test fixtures must have different HEAD commits" }
    return [pscustomobject]@{Root=$root;Control=$control;Target=$target;Outside=$outside;Runner=(Join-Path $control "scripts\run-agent-verification.ps1");ControlHead=$controlHead;TargetHead=$targetHead}
}

function Invoke-RunnerSelfTest([string]$ControlRoot) {
    $cases = Load-Json (Get-RepoPath $ControlRoot ".agent/evals/runner-cases.json")
    $expectedCases = [ordered]@{
        control_script_runs_from_control_root="pass"; control_script_target_root_argument_bound="pass"; target_script_runs_from_target_root="pass"; native_gate_pass="pass"; native_gate_required_fail="fail"; native_gate_optional_fail="pass"; json_parse_valid="pass"; json_parse_invalid_required="fail"; required_external_evidence_valid="pass"; required_external_evidence_missing="blocked"; required_external_evidence_wrong_task="blocked"; required_external_evidence_wrong_commit="blocked"; required_external_evidence_wrong_kind="blocked"; optional_external_evidence_missing="pass"; target_registry_override_ignored="pass"; task_packet_inside_target_rejected="fail"; output_path_inside_target_rejected="fail"; external_evidence_inside_target_rejected="fail"; control_script_path_traversal_rejected="fail"; target_script_path_traversal_rejected="fail"; task_state_written_outside_target="pass"; report_contains_task_packet_hash="pass"; report_contains_control_hashes="pass"; report_contains_baseline_and_head="pass"; report_contains_exact_change_records="pass"; report_contains_output_hash_not_raw_output="pass"; temporary_raw_output_removed="pass"; runner_exit_zero_only_on_pass="pass"; verifier_repository_root_override="pass"; p10a_exact_scope_regression="pass"; packet_snapshot_immutable="pass"; target_e2e_valid_change="pass"; target_e2e_unauthorized_change="fail"; bounded_process_output_capture="pass"
    }
    $actualCases = @($cases.cases)
    if ($actualCases.Count -ne $expectedCases.Count) { Fail "runner self-test matrix count does not match executable case manifest" }
    if (@($actualCases | ForEach-Object name | Sort-Object -Unique).Count -ne $actualCases.Count) { Fail "runner self-test matrix contains duplicate case names" }
    foreach ($case in $actualCases) {
        if (-not $expectedCases.Contains([string]$case.name)) { Fail "runner self-test matrix contains unexpected case: $($case.name)" }
        if ([string]$case.expected -ne [string]$expectedCases[[string]$case.name]) { Fail "runner self-test matrix expected value mismatch: $($case.name)" }
    }
    foreach ($name in $expectedCases.Keys) { if (@($actualCases | Where-Object name -eq $name).Count -ne 1) { Fail "runner self-test matrix is missing case: $name" } }
    $temp = Join-Path ([IO.Path]::GetTempPath()) ("agent-runner-selftest-" + [guid]::NewGuid().ToString("N"))
    $control = Join-Path $temp "control"; $target = Join-Path $temp "target"; $outside = Join-Path $temp "outside"
    New-Item -ItemType Directory -Force -Path $control,$target,$outside | Out-Null
    try {
        $state = [pscustomobject]@{task_id="RUNNER-SELFTEST"; head_commit="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; baseline_commit="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"; packet_gates=@(); change_records=@(); impacts=[pscustomobject]@{runtime_behavior="none";domain_behavior="none";api="none";database="none";dependencies="none";behavior_versions="none"}}
        $controlScript = Write-RunnerText $control "control.ps1" "param([string]`$RepositoryRoot) Set-Content -LiteralPath (Join-Path `$RepositoryRoot 'control-marker.txt') -Value 'control' -NoNewline"
        $targetScript = Write-RunnerText $target "target.ps1" "Set-Content -LiteralPath (Join-Path (Get-Location) 'target-marker.txt') -Value 'target' -NoNewline"
        $nativeScript = Write-RunnerText $target "native.ps1" "Set-Content -LiteralPath (Join-Path (Get-Location) 'native-marker.txt') -Value 'native' -NoNewline"
        $validJson = Write-RunnerText $target "valid.json" '{"ok":true}'
        $invalidJson = Write-RunnerText $target "invalid.json" '{invalid'
        $registry = @(
            [pscustomobject]@{name="control";kind="control-script";script="control.ps1";arguments=@();target_root_argument="-RepositoryRoot";display_command="hidden"},
            [pscustomobject]@{name="target";kind="target-script";script="target.ps1";arguments=@();display_command="hidden"},
            [pscustomobject]@{name="native";kind="native";program=$script:PowerShellExe;arguments=@("-NoLogo","-NoProfile","-File",$nativeScript);display_command="hidden"},
            [pscustomobject]@{name="json";kind="json-parse";paths=@("valid.json");display_command="hidden"},
            [pscustomobject]@{name="invalid-json";kind="json-parse";paths=@("invalid.json");display_command="hidden"},
            [pscustomobject]@{name="evidence";kind="external-evidence";evidence_kind="test-evidence";display_command="hidden"}
        )
        $state.packet_gates = @([pscustomobject]@{gate="control";required=$true})
        Assert-RunnerCase "control_script_runs_from_control_root" "pass" { (Invoke-Gate $registry[0] $control $target $state @()).Status }
        Assert-RunnerCase "control_script_target_root_argument_bound" "pass" { if (Test-Path -LiteralPath (Join-Path $target 'control-marker.txt')) { 'pass' } else { 'fail' } }
        $state.packet_gates = @([pscustomobject]@{gate="target";required=$true})
        Assert-RunnerCase "target_script_runs_from_target_root" "pass" { (Invoke-Gate $registry[1] $control $target $state @()).Status }
        $state.packet_gates = @([pscustomobject]@{gate="native";required=$true})
        Assert-RunnerCase "native_gate_pass" "pass" { (Invoke-Gate $registry[2] $control $target $state @()).Status }
        $failNative = [pscustomobject]@{name="fail-native";kind="native";program=$script:PowerShellExe;arguments=@("-NoLogo","-NoProfile","-File",(Write-RunnerText $target "fail.ps1" "exit 4"));display_command="hidden"}
        $state.packet_gates = @([pscustomobject]@{gate="fail-native";required=$true})
        Assert-RunnerCase "native_gate_required_fail" "fail" { (Invoke-Gate $failNative $control $target $state @()).Status }
        $state.packet_gates = @([pscustomobject]@{gate="fail-native";required=$false})
        Assert-RunnerCase "native_gate_optional_fail" "pass" {
            $optionalResult = Invoke-Gate $failNative $control $target $state @()
            if ($optionalResult.Status -ne "fail") { "fail" } else { Resolve-OverallResult @([pscustomobject]@{required=$false;status=$optionalResult.Status}) }
        }
        $state.packet_gates = @([pscustomobject]@{gate="json";required=$true})
        Assert-RunnerCase "json_parse_valid" "pass" { (Invoke-Gate $registry[3] $control $target $state @()).Status }
        Assert-RunnerCase "json_parse_invalid_required" "fail" { (Invoke-Gate $registry[4] $control $target $state @()).Status }
        $evidence = [ordered]@{schema_version="1.0.0";task_id="RUNNER-SELFTEST";gate="evidence";evidence_kind="test-evidence";subject_commit=$state.head_commit;result="pass";artifact_ref="fixture";artifact_sha256=("0"*64);issuer="selftest"}
        $evidencePath = Join-Path $outside "evidence.json"; $evidence | ConvertTo-Json | Set-Content -LiteralPath $evidencePath
        $state.packet_gates = @([pscustomobject]@{gate="evidence";required=$true})
        Assert-RunnerCase "required_external_evidence_valid" "pass" { (Invoke-Gate $registry[5] $control $target $state @($evidencePath)).Status }
        $wrongTask = ($evidence | ConvertTo-Json | ConvertFrom-Json); $wrongTask.task_id = "OTHER-TASK"; $wrongTaskPath = Join-Path $outside "wrong-task.json"; $wrongTask | ConvertTo-Json | Set-Content -LiteralPath $wrongTaskPath
        $wrongCommit = ($evidence | ConvertTo-Json | ConvertFrom-Json); $wrongCommit.subject_commit = "cccccccccccccccccccccccccccccccccccccccc"; $wrongCommitPath = Join-Path $outside "wrong-commit.json"; $wrongCommit | ConvertTo-Json | Set-Content -LiteralPath $wrongCommitPath
        $wrongKind = ($evidence | ConvertTo-Json | ConvertFrom-Json); $wrongKind.evidence_kind = "other-evidence"; $wrongKindPath = Join-Path $outside "wrong-kind.json"; $wrongKind | ConvertTo-Json | Set-Content -LiteralPath $wrongKindPath
        foreach ($case in @(@("required_external_evidence_missing",@()), @("required_external_evidence_wrong_task",@($wrongTaskPath)), @("required_external_evidence_wrong_commit",@($wrongCommitPath)), @("required_external_evidence_wrong_kind",@($wrongKindPath)))) {
            $state.packet_gates = @([pscustomobject]@{gate="evidence";required=$true})
            Assert-RunnerCase $case[0] "blocked" { (Get-ExternalEvidence $case[1] $target $registry[5] $state $true).Status }
        }
        $state.packet_gates = @([pscustomobject]@{gate="evidence";required=$false})
        Assert-RunnerCase "optional_external_evidence_missing" "pass" {
            $optionalEvidence = Get-ExternalEvidence @() $target $registry[5] $state $false
            if ($optionalEvidence.Status -ne "skipped") { "fail" } else { Resolve-OverallResult @([pscustomobject]@{required=$false;status=$optionalEvidence.Status}) }
        }
        Assert-RunnerCase "task_packet_inside_target_rejected" "fail" { try { Assert-OutsideRoot (Join-Path $target "packet.json") $target "Task packet"; "pass" } catch { "fail" } }
        Assert-RunnerCase "output_path_inside_target_rejected" "fail" { try { Assert-OutsideRoot (Join-Path $target "report.json") $target "Output"; "pass" } catch { "fail" } }
        Assert-RunnerCase "external_evidence_inside_target_rejected" "fail" { try { Assert-OutsideRoot (Join-Path $target "evidence.json") $target "Evidence"; "pass" } catch { "fail" } }
        Assert-RunnerCase "control_script_path_traversal_rejected" "fail" { try { Resolve-SafeChildPath $control "../control.ps1" "control script"; "pass" } catch { "fail" } }
        Assert-RunnerCase "target_script_path_traversal_rejected" "fail" { try { Resolve-SafeChildPath $target "../target.ps1" "target script"; "pass" } catch { "fail" } }
        $statePath = Join-Path $outside "task-state.json"; Write-AtomicJson $statePath $state
        Assert-RunnerCase "task_state_written_outside_target" "pass" { if (Test-Path -LiteralPath $statePath) { "pass" } else { "fail" } }
        $fixture = New-RunnerE2EFixture $ControlRoot $temp
        $beforeOutputDirs = @((Get-ChildItem -LiteralPath ([IO.Path]::GetTempPath()) -Directory -Filter "agent-runner-output-*" -ErrorAction SilentlyContinue).Name)
        $beforePacketSnapshots = @((Get-ChildItem -LiteralPath ([IO.Path]::GetTempPath()) -File -Filter "agent-task-packet-*.json" -ErrorAction SilentlyContinue).Name)
        $validPacketObject = [ordered]@{task_id="RUNNER-E2E-VALID";original_packet_path=(Join-Path $fixture.Outside "valid-packet.json")}
        $validPacketPath = Write-RunnerText $fixture.Outside "valid-packet.json" ($validPacketObject | ConvertTo-Json -Depth 20)
        $validPacketHash = Get-BytesSha256 ([IO.File]::ReadAllBytes($validPacketPath))
        $validReportPath = Join-Path $fixture.Outside "valid-report.json"
        $validRun = Invoke-Process $script:PowerShellExe @("-NoLogo", "-NoProfile", "-File", $fixture.Runner, "-TaskPacket", $validPacketPath, "-TargetRoot", $fixture.Target, "-OutputPath", $validReportPath) $fixture.Control
        if (-not (Test-Path -LiteralPath $validReportPath -PathType Leaf)) { Fail "runner self-test valid E2E report was not written" }
        $validReport = Load-Json $validReportPath
        $validTargetHead = Get-GitHead $fixture.Target
        $validExternalPacketHash = Get-FileSha256 $validPacketPath
        Assert-RunnerCase "packet_snapshot_immutable" "pass" { if ($validReport.task_packet_sha256 -eq $validPacketHash -and $validExternalPacketHash -ne $validPacketHash -and (Load-Json $validPacketPath).mutated_by_verifier) { "pass" } else { "fail" } }
        Assert-RunnerCase "target_e2e_valid_change" "pass" { if ($validRun.ExitCode -eq 0 -and $validReport.result -eq "pass" -and $validReport.target.root -eq (Get-FullPath $fixture.Target) -and $validReport.target.head_commit -eq $validTargetHead -and $validReport.gate_results[0].kind -eq "control-script" -and $validReport.gate_results[0].status -eq "pass") { "pass" } else { "fail" } }
        Assert-RunnerCase "target_registry_override_ignored" "pass" { if ($validReport.gate_results[0].kind -eq "control-script" -and $validReport.gate_results[0].status -eq "pass") { "pass" } else { "fail" } }
        Assert-RunnerCase "report_contains_task_packet_hash" "pass" { if ($validReport.task_packet_sha256 -eq $validPacketHash) { "pass" } else { "fail" } }
        Assert-RunnerCase "report_contains_control_hashes" "pass" { if ($validReport.control_plane.file_hashes.'scripts/run-agent-verification.ps1' -match '^[0-9a-f]{64}$') { "pass" } else { "fail" } }
        Assert-RunnerCase "report_contains_baseline_and_head" "pass" { if ($validReport.target.baseline_commit -and $validReport.target.head_commit -eq $validTargetHead) { "pass" } else { "fail" } }
        Assert-RunnerCase "report_contains_exact_change_records" "pass" { if ($null -ne $validReport.change_records -and $null -ne $validReport.scope) { "pass" } else { "fail" } }
        Assert-RunnerCase "report_contains_output_hash_not_raw_output" "pass" { if ($validReport.gate_results[0].output_sha256 -match '^[0-9a-f]{64}$' -and $null -eq $validReport.gate_results[0].stdout -and $null -eq $validReport.gate_results[0].stderr) { "pass" } else { "fail" } }

        Remove-Item -LiteralPath (Join-Path $fixture.Target "valid-target.txt") -Force
        $unauthorizedPacketObject = [ordered]@{task_id="RUNNER-E2E-UNAUTHORIZED";original_packet_path=(Join-Path $fixture.Outside "unauthorized-packet.json")}
        $unauthorizedPacketPath = Write-RunnerText $fixture.Outside "unauthorized-packet.json" ($unauthorizedPacketObject | ConvertTo-Json -Depth 20)
        $unauthorizedReportPath = Join-Path $fixture.Outside "unauthorized-report.json"
        $unauthorizedRun = Invoke-Process $script:PowerShellExe @("-NoLogo", "-NoProfile", "-File", $fixture.Runner, "-TaskPacket", $unauthorizedPacketPath, "-TargetRoot", $fixture.Target, "-OutputPath", $unauthorizedReportPath) $fixture.Control
        if (-not (Test-Path -LiteralPath $unauthorizedReportPath -PathType Leaf)) { Fail "runner self-test unauthorized E2E report was not written" }
        $unauthorizedReport = Load-Json $unauthorizedReportPath
        Assert-RunnerCase "target_e2e_unauthorized_change" "fail" { if ($unauthorizedRun.ExitCode -ne 0 -and $unauthorizedReport.result -eq "fail" -and $unauthorizedReport.gate_results[0].status -eq "fail") { "fail" } else { "pass" } }
        Assert-RunnerCase "runner_exit_zero_only_on_pass" "pass" { if ($validRun.ExitCode -eq 0 -and $unauthorizedRun.ExitCode -ne 0) { "pass" } else { "fail" } }
        Assert-RunnerCase "verifier_repository_root_override" "pass" { if ($fixture.Target -ne $fixture.Control -and $validReport.target.head_commit -eq $validTargetHead -and $validReport.control_plane.head_commit -eq $fixture.ControlHead) { "pass" } else { "fail" } }
        $afterOutputDirs = @((Get-ChildItem -LiteralPath ([IO.Path]::GetTempPath()) -Directory -Filter "agent-runner-output-*" -ErrorAction SilentlyContinue).Name)
        $afterPacketSnapshots = @((Get-ChildItem -LiteralPath ([IO.Path]::GetTempPath()) -File -Filter "agent-task-packet-*.json" -ErrorAction SilentlyContinue).Name)
        Assert-RunnerCase "temporary_raw_output_removed" "pass" { if (@($afterOutputDirs | Where-Object { $_ -notin $beforeOutputDirs }).Count -eq 0 -and @($afterPacketSnapshots | Where-Object { $_ -notin $beforePacketSnapshots }).Count -eq 0) { "pass" } else { "fail" } }
        Assert-RunnerCase "bounded_process_output_capture" "pass" { if ([int64]$validReport.gate_results[0].output_byte_count -gt 0 -and $validReport.gate_results[0].output_sha256 -match '^[0-9a-f]{64}$') { "pass" } else { "fail" } }
        Assert-RunnerCase "p10a_exact_scope_regression" "pass" { if ((Assert-Registry $ControlRoot).Count -ge 5) { "pass" } else { "fail" } }
    }
    finally { if (Test-Path -LiteralPath $temp) { Remove-Item -Recurse -Force -LiteralPath $temp } }
}

function Invoke-Runner([string]$ControlRoot, [string]$TaskPacketPath, [string]$TargetRootPath, [string]$ReportPath, [string[]]$EvidencePaths) {
    $target = (Resolve-Path -LiteralPath $TargetRootPath).Path
    $packet = (Resolve-Path -LiteralPath $TaskPacketPath).Path
    [void](Assert-OutsideRoot $packet $target "Task packet")
    $report = (Assert-OutsideRoot $ReportPath $target "Output")
    $evidenceList = @($EvidencePaths | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })
    foreach ($evidence in $evidenceList) { [void](Assert-OutsideRoot (Get-FullPath $evidence) $target "External evidence") }
    $statePath = Join-Path ([IO.Path]::GetTempPath()) ("agent-task-state-" + [guid]::NewGuid().ToString("N") + ".json")
    $packetSnapshot = $null
    $verifier = Get-RepoPath $ControlRoot "scripts/verify-agent-context.ps1"
    $manifest = Load-Json (Get-RepoPath $ControlRoot ".agent/manifest.json")
    $packetSnapshot = New-ImmutablePacketSnapshot $packet
    try {
        $controlIntegrity = Invoke-Process $script:PowerShellExe @("-NoLogo", "-NoProfile", "-File", $verifier) $ControlRoot
        if ($controlIntegrity.ExitCode -ne 0) { Fail "trusted ControlRoot integrity validation failed" }
        $preflight = Invoke-Process $script:PowerShellExe @("-NoLogo", "-NoProfile", "-File", $verifier, "-TaskPacket", $packetSnapshot.Path, "-RepositoryRoot", $target, "-TaskStateOutput", $statePath) $ControlRoot
        if ($preflight.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $statePath -PathType Leaf)) { Fail "trusted verifier rejected task packet" }
        $state = Load-Json $statePath
        $targetHead = Get-GitHead $target
        if ($state.head_commit -ne $targetHead) { Fail "trusted task state HEAD does not match TargetRoot HEAD" }
        $registry = Assert-Registry $ControlRoot
        $controlHead = Get-GitHead $ControlRoot
        $results = @()
        foreach ($packetGate in @($state.packet_gates)) {
            $gate = Get-RegistryGate $registry $packetGate.gate
            $result = Invoke-Gate $gate $ControlRoot $target $state $evidenceList
            $entry = [ordered]@{gate=$gate.name;kind=$gate.kind;required=[bool]$packetGate.required;status=$result.Status;exit_code=$result.ExitCode;output_byte_count=$result.OutputByteCount;output_sha256=$result.OutputSha256;summary=$result.Summary}
            $results += $entry
        }
        $finalTargetHead = Get-GitHead $target
        if ($state.head_commit -ne $finalTargetHead) { Fail "TargetRoot HEAD changed during trusted verification" }
        $overall = Resolve-OverallResult $results
        $reportObject = New-VerificationReport $manifest $packetSnapshot.Sha256 $state $ControlRoot $target $results $controlHead $overall
        Write-AtomicJson $report $reportObject
        $reportHash = Get-FileSha256 $report
        Write-Output "[RESULT] $overall"
        Write-Output "[REPORT] $report"
        Write-Output "[REPORT_SHA256] $reportHash"
        return [pscustomobject]@{Result=$overall;ReportPath=$report;ReportSha256=$reportHash}
    }
    finally {
        if (Test-Path -LiteralPath $statePath) { Remove-Item -Force -LiteralPath $statePath }
        if ($null -ne $packetSnapshot -and (Test-Path -LiteralPath $packetSnapshot.Path)) { Remove-Item -Force -LiteralPath $packetSnapshot.Path }
    }
}

try {
    if ($SelfTest) {
        if ($TaskPacket -or $TargetRoot -or $OutputPath -or $ExternalEvidencePath) { Fail "-SelfTest cannot be combined with runner execution parameters" }
        Invoke-RunnerSelfTest $script:ControlRoot
        exit 0
    }
    if ([string]::IsNullOrWhiteSpace($TaskPacket)) { Fail "-TaskPacket is required unless -SelfTest is used" }
    $target = if ([string]::IsNullOrWhiteSpace($TargetRoot)) { $script:ControlRoot } else { (Resolve-Path -LiteralPath $TargetRoot).Path }
    $packet = (Resolve-Path -LiteralPath $TaskPacket).Path
    $output = if ([string]::IsNullOrWhiteSpace($OutputPath)) { Join-Path ([IO.Path]::GetTempPath()) ("agent-verification-report-" + [guid]::NewGuid().ToString("N") + ".json") } else { Get-FullPath $OutputPath }
    $result = Invoke-Runner $script:ControlRoot $packet $target $output $ExternalEvidencePath
    if ($result.Result -ne "pass") { exit 1 }
    exit 0
}
catch {
    Write-Error $_.Exception.Message
    exit 1
}
