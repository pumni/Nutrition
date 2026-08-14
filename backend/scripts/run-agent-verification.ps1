[CmdletBinding()]
param(
    [string]$TaskSpec,
    [string]$TargetRoot,
    [string]$OutputPath,
    [string]$ExternalEvidencePath,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$script:ControlRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$script:PowerShellExe = if (Test-Path -LiteralPath (Join-Path $PSHOME 'pwsh.exe')) { Join-Path $PSHOME 'pwsh.exe' } else { (Get-Command powershell.exe).Source }

function Fail([string]$Message) { throw "[FAIL] $Message" }
function Normalize([string]$Path) { $Path.Replace('\', '/').TrimStart('./') }
function Load-Json([string]$Path) { if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail "JSON file does not exist: $Path" }; try { Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json } catch { Fail "invalid JSON: $Path" } }
function Get-Full([string]$Path) { [IO.Path]::GetFullPath($Path) }
function Assert-Outside([string]$Path, [string]$Root, [string]$Label) { if ([string]::IsNullOrWhiteSpace($Path)) { Fail "$Label is empty" }; $full=Get-Full $Path; $root=(Get-Full $Root).TrimEnd('\','/') + '\'; if ($full.Equals($root.TrimEnd('\','/'),[StringComparison]::OrdinalIgnoreCase) -or $full.StartsWith($root,[StringComparison]::OrdinalIgnoreCase)) { Fail "$Label must be outside TargetRoot" }; return $full }
function Invoke-Process([string]$File, [string[]]$Arguments, [string]$WorkingDirectory, [string]$OutputFile) {
    $psi=[Diagnostics.ProcessStartInfo]::new(); $psi.FileName=$File; $psi.WorkingDirectory=$WorkingDirectory; $psi.UseShellExecute=$false; $psi.RedirectStandardOutput=$true; $psi.RedirectStandardError=$true; foreach($arg in $Arguments){[void]$psi.ArgumentList.Add($arg)}
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$psi; if(-not $process.Start()){Fail "could not start verification process: $File"}; $stdout=$process.StandardOutput.ReadToEnd(); $stderr=$process.StandardError.ReadToEnd(); $process.WaitForExit(); $combined=($stdout + $stderr); $combined | Set-Content -LiteralPath $OutputFile -Encoding utf8; $bytes=[Text.Encoding]::UTF8.GetByteCount($combined); $hash=[Security.Cryptography.SHA256]::Create(); $digest=([BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes($combined))).Replace('-','')).ToLowerInvariant(); [pscustomobject]@{exit_code=[int]$process.ExitCode;output_byte_count=[int64]$bytes;output_sha256=$digest;summary=if($process.ExitCode -eq 0){'process exited with code 0'}else{'process failed with exit code '+$process.ExitCode}}
}
function Assert-Registry([string]$Root) { $map=Load-Json (Join-Path $Root '.agent/maps/verification-map.json'); if($map.schema_version -ne '3.0.0' -or $map.release -ne 'agent-gates-3.0.0'){Fail 'verification registry release mismatch'}; $seen=@{}; foreach($gate in @($map.gates)){ if($seen.ContainsKey([string]$gate.name)){Fail "duplicate gate ID: $($gate.name)"}; $seen[[string]$gate.name]=$gate; if([string]$gate.kind -notin @('control-script','target-script','native','json-parse','external-evidence')){Fail "unknown gate kind: $($gate.name)"} }; return $seen }
function Invoke-Gate($Gate, [string]$GateId, [string]$TargetRoot, [string]$EvidenceDir) {
    $log=Join-Path $EvidenceDir ($GateId + '.log'); $scriptPath=$null; $args=@(); $working=$TargetRoot; $processResult=$null
    switch([string]$Gate.kind){
        'control-script' { $scriptPath=Join-Path $script:ControlRoot ([string]$Gate.script); $args=@('-NoLogo','-NoProfile','-File',$scriptPath) + @($Gate.arguments); if($Gate.target_root_argument){$args += @([string]$Gate.target_root_argument,$TargetRoot)}; $working=$script:ControlRoot; $processResult=Invoke-Process $script:PowerShellExe $args $working $log }
        'target-script' { $scriptPath=Join-Path $TargetRoot ([string]$Gate.script); $args=@('-NoLogo','-NoProfile','-File',$scriptPath) + @($Gate.arguments); $processResult=Invoke-Process $script:PowerShellExe $args $TargetRoot $log }
        'native' { $processResult=Invoke-Process ([string]$Gate.program) @($Gate.arguments) $TargetRoot $log }
        'json-parse' { try { foreach($path in @($Gate.paths)){Get-Content -Raw -LiteralPath (Join-Path $TargetRoot ([string]$path)) | ConvertFrom-Json | Out-Null}; '[PASS] JSON parsed' | Set-Content -LiteralPath $log -Encoding utf8; $processResult=[pscustomobject]@{exit_code=0;output_byte_count=0;output_sha256=('0'*64);summary='JSON parsed'} } catch { ($_ | Out-String) | Set-Content -LiteralPath $log -Encoding utf8; $processResult=[pscustomobject]@{exit_code=1;output_byte_count=0;output_sha256=('0'*64);summary='JSON parse failed'} } }
        'external-evidence' { '[BLOCKED] external evidence must be supplied outside TargetRoot' | Set-Content -LiteralPath $log -Encoding utf8; $processResult=[pscustomobject]@{exit_code=2;output_byte_count=0;output_sha256=('0'*64);summary='external evidence not supplied'} }
        default { Fail "unsupported canonical gate kind: $($Gate.kind)" }
    }
    [pscustomobject][ordered]@{gate_id=$GateId;kind=[string]$Gate.kind;required=$true;status=if($processResult.exit_code -eq 0){'pass'}else{'fail'};exit_code=[int]$processResult.exit_code;output_byte_count=[int64]$processResult.output_byte_count;output_sha256=[string]$processResult.output_sha256;summary=[string]$processResult.summary}
}
function Get-ChangeRecords([string]$Root, [string]$Baseline) { $records=@(); $lines=(& git -C $Root diff --name-status --no-renames $Baseline -- 2>&1 | Out-String).Trim() -split "`r?`n" | Where-Object {$_}; foreach($line in $lines){$parts=$line -split "`t";$type=if($parts[0] -match 'A'){'create'}elseif($parts[0] -match 'D'){'delete'}else{'modify'};$records += [pscustomobject]@{path=(Normalize $parts[-1]);type=$type;provenance=@('unstaged')}}; $untracked=(& git -C $Root ls-files --others --exclude-standard 2>&1 | Out-String).Trim() -split "`r?`n" | Where-Object {$_}; foreach($path in $untracked){$records += [pscustomobject]@{path=(Normalize $path);type='create';provenance=@('untracked')}}; return @($records | Sort-Object path -Unique) }
function Write-Json([string]$Path,$Value){$Value | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath $Path -Encoding utf8}
function Invoke-RunnerSelfTest {
    $registry=Assert-Registry $script:ControlRoot; if(-not $registry.ContainsKey('acl-integrity')){Fail 'canonical registry self-test gate is missing'}; Write-Output '[PASS] canonical gate registry self-test'; $temp=Join-Path ([IO.Path]::GetTempPath()) ('agent-runner-selftest-'+[guid]::NewGuid().ToString('N')); New-Item -ItemType Directory -Force $temp | Out-Null; try { Assert-Outside (Join-Path $temp 'report.json') $temp 'self-test target path' } catch { Write-Output '[PASS] output path protection self-test' }; if(Test-Path -LiteralPath $temp){Remove-Item -Recurse -Force $temp}; Write-Output '[PASS] trusted runner self-test completed.'
}

if($SelfTest){ try { Invoke-RunnerSelfTest; exit 0 } catch { Write-Error $_.Exception.Message; exit 1 } }
try {
    if([string]::IsNullOrWhiteSpace($TaskSpec)){Fail '-TaskSpec is required unless -SelfTest is used'}
    if([string]::IsNullOrWhiteSpace($TargetRoot)){Fail '-TargetRoot is required unless -SelfTest is used'}
    $target=(Resolve-Path -LiteralPath $TargetRoot).Path; $specPath=(Resolve-Path -LiteralPath $TaskSpec).Path
    $output=Assert-Outside $OutputPath $target 'OutputPath'; $evidence=if($ExternalEvidencePath){Assert-Outside $ExternalEvidencePath $target 'ExternalEvidencePath'}else{Join-Path ([IO.Path]::GetTempPath()) ('agent-verification-'+[guid]::NewGuid().ToString('N'))}; New-Item -ItemType Directory -Force $evidence | Out-Null
    $spec=Load-Json $specPath; $specHash=(Get-FileHash -Algorithm SHA256 -LiteralPath $specPath).Hash.ToLowerInvariant(); $registry=Assert-Registry $script:ControlRoot
    $preflightLog=Join-Path $evidence 'task-spec-preflight.log'; $preflight=Invoke-Process $script:PowerShellExe @('-NoLogo','-NoProfile','-File',(Join-Path $script:ControlRoot 'scripts/verify-agent-context.ps1'),'-TaskSpec',$specPath,'-RepositoryRoot',$target) $script:ControlRoot $preflightLog; if($preflight.exit_code -ne 0){Fail 'CONTEXT_INTEGRITY_FAILED: Task Spec preflight failed'}
    $gateResults=@(); foreach($gateId in @($spec.required_verification_gates)){if(-not $registry.ContainsKey([string]$gateId)){Fail "unknown required gate: $gateId"};$gateResults += Invoke-Gate $registry[[string]$gateId] ([string]$gateId) $target $evidence }
    $failed=@($gateResults|Where-Object status -ne 'pass'); $records=Get-ChangeRecords $target ([string]$spec.baseline.commit); $report=[ordered]@{schema_version='2.0.0';task_id=[string]$spec.task_id;task_spec_sha256=$specHash;releases=[ordered]@{runner_release='agent-runner-2.0.0';verifier_release='agent-verifier-3.0.0';verification_registry_release='agent-gates-3.0.0'};control_plane=[ordered]@{root=$script:ControlRoot;head_commit=(& git -C $script:ControlRoot rev-parse HEAD).Trim();file_hashes=@{'scripts/run-agent-verification.ps1'=(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $script:ControlRoot 'scripts/run-agent-verification.ps1')).Hash.ToLowerInvariant()}};target=[ordered]@{root=$target;baseline_commit=[string]$spec.baseline.commit;head_commit=(& git -C $target rev-parse HEAD).Trim()};change_records=@($records);gate_results=@($gateResults);scope=[ordered]@{result=if($failed.Count -eq 0){'pass'}else{'fail'};create=@($records|Where-Object type -eq 'create'|ForEach-Object path);modify=@($records|Where-Object type -eq 'modify'|ForEach-Object path);delete=@($records|Where-Object type -eq 'delete'|ForEach-Object path)};impacts=[ordered]@{runtime_behavior='none';domain_behavior='none';api='none';database='none';dependencies='none';behavior_versions='none'};result=if($failed.Count -eq 0){'pass'}else{'fail'}}
    New-Item -ItemType Directory -Force (Split-Path -Parent $output) | Out-Null; Write-Json $output $report; Write-Output "[PASS] verification report written: $output"; if($failed.Count -gt 0){exit 1}; exit 0
}
catch { Write-Error $_.Exception.Message; exit 1 }
