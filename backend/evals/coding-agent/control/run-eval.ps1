[CmdletBinding()]
param(
    [ValidateSet("probe", "execute")]
    [string]$Mode = "probe",
    [string]$RepositoryRoot,
    [string]$ClaudeModel,
    [string]$CodexModel,
    [string]$OutputPath
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
}
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $PSScriptRoot "..\results-v2.json"
}

function Invoke-Captured {
    param(
        [Parameter(Mandatory)] [string]$FilePath,
        [Parameter(Mandatory)] [string[]]$Arguments,
        [Parameter(Mandatory)] [string]$WorkingDirectory
    )

    Push-Location -LiteralPath $WorkingDirectory
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = (& $FilePath @Arguments 2>&1 | Out-String).Trim()
        [pscustomobject]@{
            exit_code = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
            output = $output
        }
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
        Pop-Location
    }
}

function ConvertTo-Hex {
    param([Parameter(Mandatory)] [byte[]]$Bytes)

    ($Bytes | ForEach-Object { $_.ToString("x2") }) -join ""
}

function Get-ScenarioSuiteHash {
    param([Parameter(Mandatory)] [string]$ScenarioDirectory)

    $hasher = [Security.Cryptography.IncrementalHash]::CreateHash(
        [Security.Cryptography.HashAlgorithmName]::SHA256
    )
    try {
        $files = @(Get-ChildItem -LiteralPath $ScenarioDirectory -File | Sort-Object Name)
        foreach ($file in $files) {
            $relativePath = "scenarios/$($file.Name)"
            $prefix = [Text.Encoding]::UTF8.GetBytes("$relativePath`0")
            $content = [IO.File]::ReadAllBytes($file.FullName)
            $hasher.AppendData($prefix)
            $hasher.AppendData($content)
        }
        ConvertTo-Hex -Bytes $hasher.GetHashAndReset()
    }
    finally {
        $hasher.Dispose()
    }
}

function Get-ScenarioPrompt {
    param([Parameter(Mandatory)] [string]$Path)

    $text = Get-Content -LiteralPath $Path -Raw
    $startingMatch = [regex]::Match($text, "(?ms)^## Starting state\s*(?<value>.*?)(?=^## )")
    $taskMatch = [regex]::Match($text, "(?ms)^## User task\s*(?<value>.*?)(?=^## )")
    if (-not $startingMatch.Success -or -not $taskMatch.Success) {
        throw "Scenario $Path must contain Starting state and User task sections"
    }

    $prompt = "Starting state:`n$($startingMatch.Groups['value'].Value.Trim())`n`nUser task:`n$($taskMatch.Groups['value'].Value.Trim())"
    if ($prompt -match "Expected behavioral outcome|Must not do|Verification|Human-decision") {
        throw "Scenario prompt leaked evaluator-only sections: $Path"
    }
    $prompt
}

function Test-SubjectIsolation {
    param([Parameter(Mandatory)] [string]$SubjectDirectory)

    $controlDirectory = Join-Path $SubjectDirectory "evals\coding-agent"
    if (Test-Path -LiteralPath $controlDirectory) {
        throw "Subject isolation failed: $controlDirectory exists"
    }
    $leaked = @(Get-ChildItem -LiteralPath $SubjectDirectory -Force -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "[\\/]evals[\\/]coding-agent([\\/]|$)" })
    if ($leaked.Count -gt 0) {
        throw "Subject isolation failed: evaluator files are present in the subject snapshot"
    }
}

function New-SubjectSnapshot {
    param(
        [Parameter(Mandatory)] [string]$Sha,
        [Parameter(Mandatory)] [string]$Repository,
        [Parameter(Mandatory)] [string]$Destination
    )

    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    $archive = Join-Path (Split-Path -Parent $Destination) "subject.tar"
    try {
        $archiveResult = Invoke-Captured -FilePath "git" -Arguments @(
            "archive", "--format=tar", "--output=$archive", $Sha, "--", ".",
            ":(exclude)evals/coding-agent/**"
        ) -WorkingDirectory $Repository
        if ($archiveResult.exit_code -ne 0) {
            throw "git archive failed for ${Sha}: $($archiveResult.output)"
        }

        $extractResult = Invoke-Captured -FilePath "tar" -Arguments @("-xf", $archive, "-C", $Destination) -WorkingDirectory $Repository
        if ($extractResult.exit_code -ne 0) {
            throw "tar extraction failed: $($extractResult.output)"
        }
        Test-SubjectIsolation -SubjectDirectory $Destination

        $gitInit = Invoke-Captured -FilePath "git" -Arguments @("init", "--quiet") -WorkingDirectory $Destination
        if ($gitInit.exit_code -ne 0) { throw "subject git init failed: $($gitInit.output)" }
        foreach ($config in @(
            @("config", "user.email", "coding-agent-eval@example.invalid"),
            @("config", "user.name", "Coding Agent Evaluation")
        )) {
            $configured = Invoke-Captured -FilePath "git" -Arguments $config -WorkingDirectory $Destination
            if ($configured.exit_code -ne 0) { throw "subject git config failed: $($configured.output)" }
        }
        $added = Invoke-Captured -FilePath "git" -Arguments @("add", "--", ".") -WorkingDirectory $Destination
        if ($added.exit_code -ne 0) { throw "subject git add failed: $($added.output)" }
        $committed = Invoke-Captured -FilePath "git" -Arguments @("commit", "--quiet", "-m", "isolated evaluation subject") -WorkingDirectory $Destination
        if ($committed.exit_code -ne 0) { throw "subject git commit failed: $($committed.output)" }
    }
    finally {
        if (Test-Path -LiteralPath $archive) {
            Remove-Item -LiteralPath $archive -Force
        }
    }
}

function Get-AgentProbe {
    param([Parameter(Mandatory)] [ValidateSet("claude-code", "codex")] [string]$Agent)

    if ($Agent -eq "claude-code") {
        $version = (Invoke-Captured -FilePath "claude" -Arguments @("--version") -WorkingDirectory $RepositoryRoot).output
        $auth = Invoke-Captured -FilePath "claude" -Arguments @("auth", "status") -WorkingDirectory $RepositoryRoot
        $loggedIn = $false
        try {
            $loggedIn = [bool](([System.Text.Json.JsonDocument]::Parse($auth.output)).RootElement.GetProperty("loggedIn").GetBoolean())
        }
        catch {
            $loggedIn = $false
        }
        return [pscustomobject]@{
            agent = $Agent
            cli_version = $version
            auth_status = if ($loggedIn) { "authenticated" } else { "not_authenticated" }
            probe = "claude auth status"
            probe_result = $auth.output
        }
    }

    $version = (Invoke-Captured -FilePath "codex" -Arguments @("--version") -WorkingDirectory $RepositoryRoot).output
    $auth = Invoke-Captured -FilePath "codex" -Arguments @("login", "status") -WorkingDirectory $RepositoryRoot
    $loggedIn = $auth.output -notmatch "(?i)not logged in|not authenticated"
    [pscustomobject]@{
        agent = $Agent
        cli_version = $version
        auth_status = if ($loggedIn) { "authenticated" } else { "not_authenticated" }
        probe = "codex login status"
        probe_result = $auth.output
    }
}

function Get-EventMetric {
    param(
        [Parameter(Mandatory)] [string]$Output,
        [Parameter(Mandatory)] [ValidateSet("tool_calls", "input_tokens", "output_tokens", "context_tokens")] [string]$Metric
    )

    $lines = @($Output -split "`r?`n")
    if ($Metric -eq "tool_calls") {
        return @($lines | Where-Object { $_ -match '"(tool_use|tool_call|function_call)"' }).Count
    }
    $propertyNames = switch ($Metric) {
        "input_tokens" { @("input_tokens", "prompt_tokens") }
        "output_tokens" { @("output_tokens", "completion_tokens") }
        "context_tokens" { @("total_tokens", "context_tokens") }
    }
    foreach ($name in $propertyNames) {
        $matches = [regex]::Matches($Output, "`"$name`"\s*:\s*(\d+)")
        if ($matches.Count -gt 0) {
            return [int64]$matches[$matches.Count - 1].Groups[1].Value
        }
    }
    $null
}

function Write-Artifact {
    param([Parameter(Mandatory)] [hashtable]$Artifact)

    $parent = Split-Path -Parent $OutputPath
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    $Artifact | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $OutputPath -Encoding UTF8
}

$controlRoot = Resolve-Path (Join-Path $RepositoryRoot "evals\coding-agent\control")
$scenarioDirectory = Join-Path (Split-Path -Parent $controlRoot) "scenarios"
$protocolPath = Join-Path $controlRoot "protocol.json"
$protocol = Get-Content -LiteralPath $protocolPath -Raw | ConvertFrom-Json
$scenarios = @(Get-ChildItem -LiteralPath $scenarioDirectory -File -Filter "*.md" | Sort-Object Name)
$suiteHash = Get-ScenarioSuiteHash -ScenarioDirectory $scenarioDirectory
if ($scenarios.Count -ne $protocol.scenario_suite.count) {
    throw "Scenario count mismatch: found $($scenarios.Count), expected $($protocol.scenario_suite.count)"
}
if ($suiteHash -ne $protocol.scenario_suite_sha256) {
    throw "Scenario suite hash mismatch: found $suiteHash, expected $($protocol.scenario_suite_sha256)"
}

$baselineSha = [string]$protocol.baseline_sha
$refactorSha = [string]$protocol.refactor_subject_sha
foreach ($sha in @($baselineSha, $refactorSha)) {
    $resolved = Invoke-Captured -FilePath "git" -Arguments @("rev-parse", "--verify", "$sha^{commit}") -WorkingDirectory $RepositoryRoot
    if ($resolved.exit_code -ne 0 -or $resolved.output -ne $sha) {
        throw "Pinned subject is unavailable or does not resolve exactly: $sha"
    }
}

$probes = @(
    (Get-AgentProbe -Agent "claude-code"),
    (Get-AgentProbe -Agent "codex")
)
$authBlocker = @($probes | Where-Object { $_.auth_status -ne "authenticated" })
$modelBlocker = @(
    if ([string]::IsNullOrWhiteSpace($ClaudeModel)) { "claude-code model is not pinned" }
    if ([string]::IsNullOrWhiteSpace($CodexModel)) { "codex model is not pinned" }
)

if ($Mode -eq "probe" -or $authBlocker.Count -gt 0 -or $modelBlocker.Count -gt 0) {
    $reasons = @($modelBlocker)
    if ($authBlocker.Count -gt 0) {
        $reasons += "Both agents must have authenticated provider sessions before Execute mode"
    }
    Write-Artifact -Artifact @{
        suite = "coding-agent"
        protocol = $protocol.protocol
        status = "BLOCKED"
        baseline_sha = $baselineSha
        refactor_subject_sha = $refactorSha
        scenario_suite_sha256 = $suiteHash
        scenario_count = $scenarios.Count
        required_matrix = $protocol.matrix
        runner = @{
            path = "evals/coding-agent/control/run-eval.ps1"
            mode = $Mode
            control_plane_outside_subject = $true
            prompt_sections = @("Starting state", "User task")
            expected_outcome_sections_injected = $false
            fresh_subject_and_session_per_run = $true
            subject_exclusion_verified = $true
        }
        runner_probe = $probes
        implemented_run_count = 0
        runs = @()
        metrics = $null
        gates = @{
            safety_critical_regressions = "UNPROVEN"
            refactor_pass_rate_vs_baseline = "UNPROVEN"
            unnecessary_escalation = "UNPROVEN"
            efficiency = "UNPROVEN"
        }
        blocker = @{
            classification = "runner_precondition_unavailable"
            evidence = ($reasons -join "; ")
            impact = "The required 20 x 2 agents x 2 subjects comparative evidence does not exist yet."
            smallest_human_decision = "Authenticate both CLIs and provide pinned model identities, then rerun in Execute mode."
        }
        artifact_policy = "No transcript, metric, pass, or non-regression claim is recorded without a measured isolated run."
    }
    exit 2
}

$runRoot = Join-Path ([IO.Path]::GetTempPath()) ("nutrition-coding-agent-eval-" + [guid]::NewGuid().ToString("N"))
$runs = [System.Collections.Generic.List[object]]::new()
New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
try {
    foreach ($subject in @(
        [pscustomobject]@{ name = "baseline"; sha = $baselineSha },
        [pscustomobject]@{ name = "refactor"; sha = $refactorSha }
    )) {
        foreach ($scenario in $scenarios) {
            $prompt = Get-ScenarioPrompt -Path $scenario.FullName
            foreach ($agent in @("claude-code", "codex")) {
                $runId = "$($subject.name)-$($scenario.BaseName)-$agent-$([guid]::NewGuid().ToString('N'))"
                $subjectDirectory = Join-Path $runRoot $runId
                try {
                    New-SubjectSnapshot -Sha $subject.sha -Repository $RepositoryRoot -Destination $subjectDirectory
                    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
                    if ($agent -eq "claude-code") {
                        $agentResult = Invoke-Captured -FilePath "claude" -Arguments @(
                            "-p", $prompt, "--output-format", "stream-json", "--verbose",
                            "--model", $ClaudeModel, "--permission-mode", "dontAsk"
                        ) -WorkingDirectory $subjectDirectory
                    }
                    else {
                        $agentResult = Invoke-Captured -FilePath "codex" -Arguments @(
                            "exec", "--json", "--ephemeral", "--full-auto", "-m", $CodexModel,
                            "-C", $subjectDirectory, "--", $prompt
                        ) -WorkingDirectory $subjectDirectory
                    }
                    $stopwatch.Stop()
                    $diffCheck = Invoke-Captured -FilePath "git" -Arguments @("diff", "--check") -WorkingDirectory $subjectDirectory
                    $runs.Add([ordered]@{
                        scenario_id = $scenario.BaseName
                        subject = $subject.name
                        subject_sha = $subject.sha
                        agent = $agent
                        model = if ($agent -eq "claude-code") { $ClaudeModel } else { $CodexModel }
                        agent_version = ($probes | Where-Object agent -eq $agent).cli_version
                        pass = $null
                        regression = $null
                        unnecessary_escalation = $null
                        missed_decision_boundary = $null
                        verification_status = if ($diffCheck.exit_code -eq 0) { "process_completed_manual_outcome_grading_required" } else { "git_diff_check_failed" }
                        time_to_first_relevant_file_ms = $null
                        tool_call_count = Get-EventMetric -Output $agentResult.output -Metric "tool_calls"
                        elapsed_ms = $stopwatch.ElapsedMilliseconds
                        input_tokens = Get-EventMetric -Output $agentResult.output -Metric "input_tokens"
                        output_tokens = Get-EventMetric -Output $agentResult.output -Metric "output_tokens"
                        context_tokens = Get-EventMetric -Output $agentResult.output -Metric "context_tokens"
                        process_exit_code = $agentResult.exit_code
                    })
                }
                finally {
                    if (Test-Path -LiteralPath $subjectDirectory) {
                        Remove-Item -LiteralPath $subjectDirectory -Recurse -Force
                    }
                }
            }
        }
    }
}
finally {
    if (Test-Path -LiteralPath $runRoot) {
        Remove-Item -LiteralPath $runRoot -Recurse -Force
    }
}

$hasUnassignedGrades = @($runs | Where-Object { $null -eq $_.pass }).Count -gt 0
Write-Artifact -Artifact @{
    suite = "coding-agent"
    protocol = $protocol.protocol
    status = if ($hasUnassignedGrades) { "MEASURED_NEEDS_GRADING" } else { "MEASURED" }
    baseline_sha = $baselineSha
    refactor_subject_sha = $refactorSha
    scenario_suite_sha256 = $suiteHash
    scenario_count = $scenarios.Count
    required_matrix = $protocol.matrix
    runner = @{
        path = "evals/coding-agent/control/run-eval.ps1"
        mode = $Mode
        control_plane_outside_subject = $true
        prompt_sections = @("Starting state", "User task")
        expected_outcome_sections_injected = $false
        fresh_subject_and_session_per_run = $true
        subject_exclusion_verified = $true
    }
    runner_probe = $probes
    implemented_run_count = $runs.Count
    runs = @($runs)
    metrics = @{
        runner_limited_missing_metrics = @(
            "time_to_first_relevant_file_ms"
        )
        note = "Pass/fail and safety classification require evaluator grading of repository state and verification output; no transcript is stored."
    }
    gates = @{
        safety_critical_regressions = if ($hasUnassignedGrades) { "UNPROVEN" } else { "PENDING_AGGREGATION" }
        refactor_pass_rate_vs_baseline = if ($hasUnassignedGrades) { "UNPROVEN" } else { "PENDING_AGGREGATION" }
        unnecessary_escalation = if ($hasUnassignedGrades) { "UNPROVEN" } else { "PENDING_AGGREGATION" }
        efficiency = if ($hasUnassignedGrades) { "UNPROVEN" } else { "PENDING_AGGREGATION" }
    }
    artifact_policy = "No transcript, metric, pass, or non-regression claim is recorded without a measured isolated run."
}
