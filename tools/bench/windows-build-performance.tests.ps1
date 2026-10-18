[CmdletBinding()]
param([string]$EvidenceRoot = (Join-Path ([IO.Path]::GetTempPath()) ('rotor-harness-tests-' + [Guid]::NewGuid().ToString('N'))))

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$harness = Join-Path $PSScriptRoot 'windows-build-performance.ps1'
. $harness
if (-not $IsWindows) { throw 'These process-ownership stub tests require Windows; -ValidateOnly is portable.' }
$testRoot = [IO.Path]::GetFullPath($EvidenceRoot)
if (Test-Path -LiteralPath $testRoot) { throw 'Use a new EvidenceRoot for each test invocation' }
[void][IO.Directory]::CreateDirectory($testRoot)

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$Pattern)
    $caught = $null
    try { & $Action } catch { $caught = $_ }
    if ($null -eq $caught -or $caught.Exception.Message -notmatch $Pattern) {
        throw "Expected failure matching '$Pattern'; received '$caught'"
    }
}

function Read-Journal {
    param([string]$Path)
    return @(Get-Content -LiteralPath $Path | ForEach-Object { $_ | ConvertFrom-Json -AsHashtable })
}

$savedEnvironment = @{}
foreach ($name in @('ROTOR_BENCH_TEST_MODE', 'ROTOR_BENCH_TEST_PID', 'ROTOR_BENCH_TEST_MUTATE', 'ROTOR_SIDECAR_TIMEOUT')) {
    $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$sentinel = $null
try {
    $repo = Join-Path $testRoot 'repository'
    [void][IO.Directory]::CreateDirectory((Join-Path $repo 'tools/perfcompare'))
    Copy-Item -Path (Join-Path $PSScriptRoot '../perfcompare/*.go') -Destination (Join-Path $repo 'tools/perfcompare')
    Set-Content -LiteralPath (Join-Path $repo 'go.mod') -Value "module rotor-harness-selftest`n`ngo 1.25.0"
    & git -C $repo init --quiet
    & git -C $repo add .
    & git -C $repo -c user.name=Harness -c user.email=harness@example.invalid commit --quiet -m 'test repository' -m 'Co-authored-by: Codex <noreply@openai.com>'
    if ($LASTEXITCODE -ne 0) { throw 'test repository setup failed' }
    $stub = Join-Path $testRoot 'stub.exe'
    & go build -o $stub (Join-Path $PSScriptRoot 'testdata/process-stub.go')
    if ($LASTEXITCODE -ne 0) { throw 'stub prebuild failed' }
    [void][IO.Directory]::CreateDirectory((Join-Path $testRoot 'bin'))
    $script:BaselineExe = Join-Path $testRoot 'bin/baseline.exe'
    $script:CandidateExe = Join-Path $testRoot 'bin/candidate.exe'
    Copy-Item -LiteralPath $stub -Destination $BaselineExe
    Copy-Item -LiteralPath $stub -Destination $CandidateExe
    $script:Fixture = Join-Path $testRoot 'fixture'
    [void][IO.Directory]::CreateDirectory($Fixture)
    Set-Content -LiteralPath (Join-Path $Fixture 'input.ts') -Value 'export const value = 1'
    $script:BaselineSourceSha = '1111111111111111111111111111111111111111'
    $script:CandidateSourceSha = '2222222222222222222222222222222222222222'
    $script:FixtureIdentity = 'synthetic-stub-fixture'
    $script:ColdPairs = 1
    $script:NoChangeRuns = 1
    $script:SidecarTimeout = '5s'

    # Inject slow evidence storage without delaying the compiler's own writes.
    $originalWriter = (Get-Item Function:Write-ProcessEvidence).ScriptBlock
    function Write-ProcessEvidence {
        param($StdoutPath, $StderrPath, $Stdout, $Stderr)
        Start-Sleep -Milliseconds 1000
        & $originalWriter $StdoutPath $StderrPath $Stdout $Stderr
    }
    $env:ROTOR_BENCH_TEST_MODE = 'compiler-persistence'
    $total = [Diagnostics.Stopwatch]::StartNew()
    $result = Invoke-ProcessWithTimeout $stub @('build', '--project', $Fixture) $Fixture ([TimeSpan]::FromSeconds(5)) (Join-Path $testRoot 'timing.stdout') (Join-Path $testRoot 'timing.stderr')
    $total.Stop()
    Set-Item Function:Write-ProcessEvidence $originalWriter
    Assert-True ($result.Status -eq 'completed' -and $result.DurationMS -ge 160) 'compiler persistence was excluded from duration'
    Assert-True ($result.EvidencePersistenceMS -ge 950) 'injected evidence persistence was not recorded'
    Assert-True (($total.Elapsed.TotalMilliseconds - $result.DurationMS) -ge 950) 'evidence persistence contaminated duration_ms'
    Write-Output 'PASS: compiler persistence included; delayed evidence persistence excluded and recorded'

    $env:ROTOR_BENCH_TEST_MODE = 'stream-tail'
    $result = Invoke-ProcessWithTimeout $stub @('build') $Fixture ([TimeSpan]::FromSeconds(5)) (Join-Path $testRoot 'streams.stdout') (Join-Path $testRoot 'streams.stderr')
    Assert-True ($result.Status -eq 'completed' -and $result.DurationMS -ge 220) 'redirected-stream completion was excluded'
    Assert-True ((Get-Content -LiteralPath (Join-Path $testRoot 'streams.stderr') -Raw) -match 'descendant stream tail') 'descendant stderr was lost'
    Write-Output 'PASS: duration includes inherited redirected-stream completion after parent exit'

    $journal = Join-Path $testRoot 'failure-invocations.ndjson'
    $env:ROTOR_BENCH_TEST_MODE = ''
    [void](Invoke-ScoredBuild $stub $Fixture ([TimeSpan]::FromSeconds(5)) (Join-Path $testRoot 'good') $journal @{ scored = $true })
    foreach ($mode in @('nonzero', 'invalid', 'not-ok')) {
        $env:ROTOR_BENCH_TEST_MODE = $mode
        Assert-Throws { Invoke-ScoredBuild $stub $Fixture ([TimeSpan]::FromSeconds(5)) (Join-Path $testRoot $mode) $journal @{ scored = $true } } 'build failed|JSON'
        Assert-True ((Get-Item -LiteralPath (Join-Path $testRoot "$mode.stdout.json")).Length -gt 0) "$mode stdout was lost"
        Assert-True ((Get-Item -LiteralPath (Join-Path $testRoot "$mode.stderr.txt")).Length -gt 0) "$mode stderr was lost"
    }
    $env:ROTOR_BENCH_TEST_PID = Join-Path $testRoot 'sentinel.pid'
    $sentinelInfo = [Diagnostics.ProcessStartInfo]::new($stub)
    [void]$sentinelInfo.ArgumentList.Add('--hang')
    $sentinel = [Diagnostics.Process]::Start($sentinelInfo)
    $env:ROTOR_BENCH_TEST_PID = Join-Path $testRoot 'overdue-child.pid'
    $env:ROTOR_BENCH_TEST_MODE = 'hang'
    Assert-Throws { Invoke-ScoredBuild $stub $Fixture ([TimeSpan]::FromMilliseconds(800)) (Join-Path $testRoot 'timeout') $journal @{ scored = $true } } 'timed out'
    $childPid = [int](Get-Content -LiteralPath $env:ROTOR_BENCH_TEST_PID -Raw)
    Assert-True ($null -eq (Get-Process -Id $childPid -ErrorAction SilentlyContinue)) 'overdue descendant survived job termination'
    Assert-True (-not $sentinel.HasExited) 'an unrelated stub process was terminated'
    $entries = Read-Journal $journal
    Assert-True ($entries.Count -eq 10 -and $entries[1].status -eq 'completed') 'earlier journal records were lost'
    Assert-True ($entries[-1].status -eq 'incomplete' -and $entries[-1].result.Status -eq 'timeout') 'timeout was not explicitly incomplete'
    Assert-True ((Get-Content -LiteralPath (Join-Path $testRoot 'timeout.stderr.txt') -Raw) -match 'before completion') 'partial timeout stderr was lost'
    Write-Output 'PASS: nonzero, invalid JSON, and descendant-held-stream timeout preserve evidence and append-only records; unrelated process survives'

    Assert-Throws { Invoke-ScoredBuild (Join-Path $testRoot 'missing.exe') $Fixture ([TimeSpan]::FromSeconds(5)) (Join-Path $testRoot 'missing') $journal @{ scored = $true } } 'Start|find|start'
    $entries = Read-Journal $journal
    Assert-True ($entries.Count -eq 12 -and $entries[-1].status -eq 'incomplete' -and $null -eq $entries[-1].result.ExitCode) 'launch failure lost its incomplete invocation record'
    Assert-True ((Test-Path -LiteralPath (Join-Path $testRoot 'missing.stdout.json')) -and (Test-Path -LiteralPath (Join-Path $testRoot 'missing.stderr.txt'))) 'launch failure did not preserve empty stream evidence'

    $env:ROTOR_BENCH_TEST_PID = Join-Path $testRoot 'cancelled-child.pid'
    $cancelJournal = Join-Path $testRoot 'cancelled.ndjson'
    $cancelPrefix = Join-Path $testRoot 'cancelled'
    $pipeline = [PowerShell]::Create()
    try {
        [void]$pipeline.AddScript({
            param($HarnessFile, $StubFile, $FixturePath, $Prefix, $Journal)
            . $HarnessFile
            [void](Invoke-ScoredBuild $StubFile $FixturePath ([TimeSpan]::FromSeconds(20)) $Prefix $Journal @{ scored = $true })
        }).AddArgument($harness).AddArgument($stub).AddArgument($Fixture).AddArgument($cancelPrefix).AddArgument($cancelJournal)
        $pending = $pipeline.BeginInvoke()
        $ready = [Diagnostics.Stopwatch]::StartNew()
        while (-not (Test-Path -LiteralPath $env:ROTOR_BENCH_TEST_PID) -and $ready.Elapsed.TotalSeconds -lt 5) { Start-Sleep -Milliseconds 20 }
        Assert-True (Test-Path -LiteralPath $env:ROTOR_BENCH_TEST_PID) 'cancellation stub did not become ready'
        Start-Sleep -Milliseconds 100
        $cancelTime = [Diagnostics.Stopwatch]::StartNew()
        $pipeline.Stop()
        $cancelTime.Stop()
        Assert-True ($cancelTime.Elapsed.TotalSeconds -lt 5) 'cancellation waited for the invocation timeout'
        $cancelled = Read-Journal $cancelJournal
        Assert-True ($cancelled.Count -eq 2 -and $cancelled[-1].status -eq 'incomplete') 'cancellation lost its incomplete finish record'
        Assert-True ((Get-Content -LiteralPath "$cancelPrefix.stdout.json" -Raw) -match '"ok":true') 'cancelled stdout was lost'
        Assert-True ((Get-Content -LiteralPath "$cancelPrefix.stderr.txt" -Raw) -match 'before completion') 'cancelled stderr was lost'
        $cancelledPid = [int](Get-Content -LiteralPath $env:ROTOR_BENCH_TEST_PID -Raw)
        Assert-True ($null -eq (Get-Process -Id $cancelledPid -ErrorAction SilentlyContinue)) 'cancelled owned descendant survived'
        Write-Output 'PASS: PowerShell pipeline cancellation promptly preserves stdout/stderr and incomplete records, and terminates its owned descendant'
    } finally { $pipeline.Dispose() }

    $savedPath = $env:PATH
    try {
        $env:PATH = ''
        Assert-Throws { Assert-EvaluatorPrerequisites $repo } 'requires Go'
    } finally { $env:PATH = $savedPath }
    Assert-Throws { Assert-EvaluatorPrerequisites $Fixture } 'prerequisite missing'
    # These are fabricated evaluator fixtures, never measured performance scores.
    $synthetic = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../perfcompare/testdata/pass.json') -Raw | ConvertFrom-Json -AsHashtable
    $syntheticPath = Join-Path $testRoot 'synthetic-boundaries.json'
    $syntheticVerdict = Join-Path $testRoot 'synthetic-boundaries-verdict.json'
    $synthetic | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $syntheticPath
    Push-Location $Fixture
    try {
        $evaluation = Invoke-PerfCompare $repo $syntheticPath $syntheticVerdict
        Assert-True ($evaluation.ExitCode -eq 0 -and $evaluation.Status -eq 'PASS') 'legacy inclusive threshold boundaries changed'
        foreach ($record in $synthetic.records) {
            if ($record.binary -eq 'candidate') { $record.duration_ms++ }
        }
        $synthetic | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $syntheticPath
        $evaluation = Invoke-PerfCompare $repo $syntheticPath $syntheticVerdict
        $verdict = Get-Content -LiteralPath $syntheticVerdict -Raw | ConvertFrom-Json
        Assert-True ($evaluation.ExitCode -ne 0 -and $evaluation.Status -eq 'FAIL') 'legacy threshold failure was suppressed'
        Assert-True ('cold_median_ratio_exceeded' -in $verdict.reasons.code -and 'no_change_p95_ratio_exceeded' -in $verdict.reasons.code) 'actual legacy ratio failures were lost'
    } finally { Pop-Location }
    Write-Output 'PASS: fabricated inclusive .60/1.10 boundaries pass; exceeding both preserves legacy FAIL and reason codes'
    $env:ROTOR_BENCH_TEST_MODE = ''
    $script:EvidenceDir = Join-Path $testRoot 'schema-rejection'
    Push-Location $Fixture
    try { Assert-Throws { Invoke-RealBenchmark $repo } 'invalid evidence' }
    finally { Pop-Location }
    $run = Get-Content -LiteralPath (Join-Path $EvidenceDir 'run.json') -Raw | ConvertFrom-Json -AsHashtable
    Assert-True ($run.status -eq 'incomplete' -and $run.evaluation.Status -eq 'FAIL') 'invalid sample count became completed/scored evidence'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $EvidenceDir 'windows-build-performance-manifest.json'))) 'invalid evidence got a final manifest'
    Assert-True ($run.before.baseline.provided_source_sha -cne $run.before.candidate.provided_source_sha) 'A/B source SHAs were conflated'
    Assert-True ($run.before.fixture.sha256 -eq $run.after.fixture.sha256) 'canonical fixture digest was omitted or changed'
    Assert-True ((Read-Journal (Join-Path $EvidenceDir 'invocations.ndjson')).Count -eq 10) 'scored/diagnostic invocation records missing'
    Assert-True ([Environment]::GetEnvironmentVariable('ROTOR_SIDECAR_TIMEOUT', 'Process') -ceq $savedEnvironment.ROTOR_SIDECAR_TIMEOUT) 'sidecar timeout environment was not restored'
    Write-Output 'PASS: missing Go/evaluator fail preflight; explicit repository cwd works; schema rejection remains incomplete; distinct A/B SHA provenance preserved'

    foreach ($mode in @('invalid', 'hang', 'mutate-binary', 'mutate-fixture')) {
        $script:EvidenceDir = Join-Path $testRoot "run-$mode"
        $env:ROTOR_BENCH_TEST_MODE = $mode
        $env:ROTOR_BENCH_TEST_MUTATE = if ($mode -eq 'mutate-binary') { $CandidateExe } else { Join-Path $Fixture 'input.ts' }
        $script:SidecarTimeout = if ($mode -eq 'hang') { '800ms' } else { '5s' }
        Assert-Throws { Invoke-RealBenchmark $repo } 'JSON|timed out|identity changed'
        $run = Get-Content -LiteralPath (Join-Path $EvidenceDir 'run.json') -Raw | ConvertFrom-Json -AsHashtable
        Assert-True ($run.status -eq 'incomplete' -and $null -ne $run.after) "$mode did not preserve final incomplete identity"
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $EvidenceDir 'windows-build-performance-manifest.json'))) "$mode became a scored manifest"
        Assert-True ((Read-Journal (Join-Path $EvidenceDir 'invocations.ndjson')).Count -ge 2) "$mode invocation was lost"
        if ($mode -eq 'mutate-binary') {
            Assert-True ($run.before.candidate.sha256 -ne $run.after.candidate.sha256) 'binary identity mutation was not recorded'
            Copy-Item -LiteralPath $stub -Destination $CandidateExe -Force
        }
        if ($mode -eq 'mutate-fixture') { Assert-True ($run.before.fixture.sha256 -ne $run.after.fixture.sha256) 'fixture mutation was not recorded' }
    }
    Write-Output 'PASS: interrupted/invalid runs and binary/fixture mutations retain incomplete run status, logs, and before/after identity'

    $go = Assert-EvaluatorPrerequisites $repo
    $tracked = @(@{ Root = $repo })
    $before = Get-InputIdentity $tracked $go
    & git -C $repo -c user.name=Harness -c user.email=harness@example.invalid commit --quiet --allow-empty -m 'change clean HEAD' -m 'Co-authored-by: Codex <noreply@openai.com>'
    $after = Get-InputIdentity $tracked $go
    Assert-True ($before.repositories[0].identity.status -ceq $after.repositories[0].identity.status) 'HEAD test did not retain clean status'
    Assert-Throws { Assert-InputIdentityEqual $before $after } 'identity changed'
    Assert-Throws { Assert-SafeCleanupTarget (Join-Path $testRoot '../outside') $testRoot } 'outside owned root'
    $junction = Join-Path $testRoot 'junction'
    [void](New-Item -ItemType Junction -Path $junction -Target $Fixture)
    Assert-Throws { Assert-SafeCleanupTarget (Join-Path $junction 'input.ts') $testRoot } 'reparse point'
    $script:EvidenceDir = Join-Path $junction 'new-evidence'
    Assert-Throws { Assert-BenchmarkInputs $repo } 'reparse point'
    $ownedCleanup = Join-Path $testRoot ('work-' + [Guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($ownedCleanup)
    [void](New-Item -ItemType Junction -Path (Join-Path $ownedCleanup 'external') -Target $Fixture)
    Assert-Throws { Remove-TestOwnedWorkCopies $testRoot $ownedCleanup } 'reparse point'
    Assert-True ((Test-Path -LiteralPath (Join-Path $Fixture 'input.ts')) -and (Test-Path -LiteralPath $ownedCleanup)) 'reparse cleanup refusal did not preserve inputs/work tree'
    Write-Output 'PASS: clean HEAD changes and traversal/junction cleanup escapes are rejected'
    Write-Output "Stub self-tests passed. Synthetic evidence: $testRoot"
} finally {
    if ($null -ne $sentinel) {
        if (-not $sentinel.HasExited) { $sentinel.Kill($true); $sentinel.WaitForExit() }
        $sentinel.Dispose()
    }
    foreach ($name in $savedEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process')
    }
}
