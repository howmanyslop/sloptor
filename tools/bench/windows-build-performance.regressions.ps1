# Adversarial lifecycle regressions; called by the main stub suite after setup.
function Invoke-HarnessRegressions {
    param([string]$HarnessFile, [string]$StubFile, [string]$Repository, [string]$Artifacts)

    $root = Join-Path $Artifacts 'r2-regressions'
    [void][IO.Directory]::CreateDirectory($root)
    $saved = @{}
    foreach ($name in @('ROTOR_BENCH_TEST_MODE', 'ROTOR_BENCH_TEST_PID', 'ROTOR_BENCH_TEST_DONE', 'ROTOR_BENCH_TEST_JSON', 'ROTOR_BENCH_TEST_VALUE', 'REVIEW_MODE', 'REVIEW_CHILD_PID', 'REVIEW_CHILD_DONE')) {
        $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    }
    $env:ROTOR_BENCH_TEST_MODE = ''
    $journal = Join-Path $root 'invocations.ndjson'
    try {
        # No attach operation exists. Immediate roots and child creation are
        # legal schedules inside the creation-time job, including fast exits.
        foreach ($index in 1..25) {
            [void](Invoke-ScoredBuild $StubFile $root ([TimeSpan]::FromSeconds(3)) (Join-Path $root "fast-$index") $journal @{ finding = 'C1'; scored = $false })
        }
        $env:ROTOR_BENCH_TEST_MODE = 'echo'
        $env:ROTOR_BENCH_TEST_VALUE = 'synthetic inherited value: "quotes" \ 雪'
        $nativeDirectory = Join-Path $root 'native path 雪'
        [void][IO.Directory]::CreateDirectory($nativeDirectory)
        $nativeStub = Join-Path $nativeDirectory 'stub name.exe'
        Copy-Item -LiteralPath $StubFile -Destination $nativeStub
        $arguments = @('', 'one two', 'a"b', 'trailing\', 'space and trailing\', '雪', "line`nbreak", '$(literal)')
        $echo = Invoke-ProcessWithTimeout $nativeStub $arguments $nativeDirectory ([TimeSpan]::FromSeconds(3)) (Join-Path $root 'argv.json') (Join-Path $root 'argv.stderr')
        $observed = Read-StrictJson (Join-Path $root 'argv.json')
        Assert-True ($echo.Status -eq 'completed' -and ($arguments | ConvertTo-Json -Compress) -ceq ($observed.arguments | ConvertTo-Json -Compress)) 'native creation changed arguments'
        Assert-True ($observed.cwd -eq $nativeDirectory -and $observed.value -ceq $env:ROTOR_BENCH_TEST_VALUE) 'native creation changed cwd/inherited environment'
        Write-Output 'PASS R2-C1: immediate roots complete; native arguments/cwd/environment preserved'

        # Kill only the command-owning host, after observing its incrementally
        # spooled bytes and immediate child's PID. The kernel-owned job must
        # terminate the child without a separate supervisor or tree-kill call.
        $env:ROTOR_BENCH_TEST_MODE = 'abrupt'
        $env:ROTOR_BENCH_TEST_PID = Join-Path $root 'abrupt-child.pid'
        $hostScript = Join-Path $root 'abrupt-host.ps1'
        [IO.File]::WriteAllText($hostScript, @'
param($HarnessPath, $StubPath, $CaseRoot)
. $HarnessPath
Write-RunStatus (Join-Path $CaseRoot 'abrupt-run.json') ([ordered]@{ status = 'incomplete' })
[void](Invoke-ScoredBuild $StubPath $CaseRoot ([TimeSpan]::FromSeconds(12)) (Join-Path $CaseRoot 'abrupt') (Join-Path $CaseRoot 'abrupt.ndjson') @{ scored = $false })
'@)
        $info = [Diagnostics.ProcessStartInfo]::new((Get-Command pwsh).Source)
        $info.UseShellExecute = $false
        $info.CreateNoWindow = $true
        foreach ($argument in @('-NoProfile', '-File', $hostScript, $HarnessFile, $StubFile, $root)) { [void]$info.ArgumentList.Add($argument) }
        $hostProcess = [Diagnostics.Process]::Start($info)
        $childProcess = $null
        function Read-ActiveSpool([string]$Path) {
            if (-not (Test-Path -LiteralPath $Path)) { return '' }
            $stream = [IO.FileStream]::new($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
            $reader = [IO.StreamReader]::new($stream)
            try { return $reader.ReadToEnd() } finally { $reader.Dispose() }
        }
        try {
            $ready = [Diagnostics.Stopwatch]::StartNew()
            while ($ready.Elapsed.TotalSeconds -lt 8) {
                if ((Test-Path -LiteralPath $env:ROTOR_BENCH_TEST_PID) -and
                    (Read-ActiveSpool (Join-Path $root 'abrupt.stdout.json')) -match '"ok":true' -and
                    (Read-ActiveSpool (Join-Path $root 'abrupt.stderr.txt')) -match 'ABRUPT_EVIDENCE_ALREADY_EMITTED') { break }
                Start-Sleep -Milliseconds 10
            }
            Assert-True ($ready.Elapsed.TotalSeconds -lt 8) 'abrupt-host capture did not become ready'
            $childProcess = [Diagnostics.Process]::GetProcessById([int](Get-Content -LiteralPath $env:ROTOR_BENCH_TEST_PID -Raw))
            [void]$childProcess.Handle
            $hostProcess.Kill()
            Assert-True ($hostProcess.WaitForExit(3000) -and $childProcess.WaitForExit(3000)) 'host death did not terminate the atomically owned child'
            Assert-True ([IO.File]::ReadAllText((Join-Path $root 'abrupt.stdout.json')) -match '"ok":true') 'abrupt host death lost already-spooled stdout'
            Assert-True ([IO.File]::ReadAllText((Join-Path $root 'abrupt.stderr.txt')) -match 'ABRUPT_EVIDENCE_ALREADY_EMITTED') 'abrupt host death lost already-spooled stderr'
            Assert-True (@(Read-Journal (Join-Path $root 'abrupt.ndjson')).Count -eq 1) 'abruptly stopped invocation pretended to finish'
            Assert-True ((Read-StrictJson (Join-Path $root 'abrupt-run.json')).status -eq 'incomplete') 'abrupt host death lost incomplete status'
        } finally {
            if (-not $hostProcess.HasExited) { $hostProcess.Kill(); $hostProcess.WaitForExit() }
            if ($null -ne $childProcess) {
                if (-not $childProcess.HasExited) { $childProcess.Kill(); $childProcess.WaitForExit() }
                $childProcess.Dispose()
            }
            $hostProcess.Dispose()
        }
        Write-Output 'PASS R2-C1/C5: abrupt host kill terminates immediate descendant and preserves already-spooled stdout/stderr with incomplete intent'

        foreach ($mode in @('closed-success', 'closed-private', 'closed-nonzero', 'closed-invalid', 'closed-not-ok', 'closed-timeout')) {
            $env:ROTOR_BENCH_TEST_MODE = $mode
            $env:ROTOR_BENCH_TEST_PID = Join-Path $root "$mode.pid"
            $env:ROTOR_BENCH_TEST_DONE = Join-Path $root "$mode.done"
            $prefix = Join-Path $root $mode
            if ($mode -in @('closed-success', 'closed-private')) {
                $result = Invoke-ScoredBuild $StubFile $root ([TimeSpan]::FromSeconds(3)) $prefix $journal @{ finding = 'C2/P01'; scored = $false }
                Assert-True ((Test-Path -LiteralPath $env:ROTOR_BENCH_TEST_DONE) -and $result.DurationMS -ge 320) 'successful descendant persistence escaped duration/completion'
            } else {
                $timeout = if ($mode -eq 'closed-timeout') { [TimeSpan]::FromMilliseconds(150) } else { [TimeSpan]::FromSeconds(3) }
                Assert-Throws { Invoke-ScoredBuild $StubFile $root $timeout $prefix $journal @{ finding = 'C2/P01'; scored = $false } } 'failed|JSON|timed out'
            }
            $childId = [int](Get-Content -LiteralPath $env:ROTOR_BENCH_TEST_PID -Raw)
            Assert-True ($null -eq (Get-Process -Id $childId -ErrorAction SilentlyContinue)) "$mode left a descendant alive"
            if ($mode -eq 'closed-timeout') { Assert-True (-not (Test-Path -LiteralPath $env:ROTOR_BENCH_TEST_DONE)) 'overdue child completed after timeout' }
        }
        Write-Output 'PASS R2-C2/P01: closed/private-pipe descendants complete persistence inside deadline; failures/timeouts leave no live owned member'

        $env:ROTOR_BENCH_TEST_MODE = 'raw-json'
        $invalid = @(
            '/* not public JSON */ {"ok":true,"diagnostics":[]}',
            '{"ok":true,"diagnostics":[],}',
            '{"ok":false,"ok":true,"diagnostics":[]}',
            '{"ok":true,"diagnostics":[],"diagnostics":[]}',
            '{"OK":true,"ok":false,"diagnostics":[]}',
            '{"ok":true,"diagnostics":[{"message":"a","message":"b","severity":"error"}]}',
            '{"ok":true,"diagnostics":{}}', '{"ok":true,"diagnostics":[1]}',
            '{"ok":true,"diagnostics":[]} {"ok":true,"diagnostics":[]}'
        )
        for ($index = 0; $index -lt $invalid.Count; $index++) {
            $env:ROTOR_BENCH_TEST_JSON = $invalid[$index]
            $prefix = Join-Path $root "strict-$index"
            Assert-Throws { Invoke-ScoredBuild $StubFile $root ([TimeSpan]::FromSeconds(3)) $prefix $journal @{ finding = 'P02'; scored = $false } } 'JSON|build failed'
            Assert-True ([IO.File]::ReadAllText("$prefix.stdout.json") -ceq $invalid[$index]) 'strict rejection lost raw bytes'
            $last = (Read-Journal $journal)[-1]
            Assert-True ($last.status -eq 'incomplete' -and -not $last.validation_completed) 'invalid/ambiguous JSON was completed'
        }
        foreach ($kind in @('timings', 'verdict', 'object')) {
            $path = Join-Path $root "strict-$kind.json"
            [IO.File]::WriteAllText($path, '{"schema":2,"totalMs":1,}')
            Assert-Throws { Read-StrictJson $path -Kind $kind } 'JSON|invalid|Expected'
        }
        $timingContract = Join-Path $root 'timing-contract.json'
        # Public authority: internal/compile/timings.go and cmd/rotor's timing
        # shape regression use schemaVersion (not the old test stub's schema).
        $validTiming = '{"schemaVersion":2,"ok":true,"totalMs":1,"stageSemantics":"workMs","stages":{},"counts":{},"metadata":{}}'
        [IO.File]::WriteAllText($timingContract, $validTiming)
        [void](Read-StrictJson $timingContract -Kind timings)
        foreach ($raw in @('{"schema":2,"totalMs":1}', $validTiming.Replace('"ok":true', '"ok":false'))) {
            [IO.File]::WriteAllText($timingContract, $raw)
            Assert-Throws { Read-StrictJson $timingContract -Kind timings } 'JSON|schemaVersion'
        }
        Write-Output 'PASS R2-P02: comments/trailing commas/duplicate fields/wrong shapes/extra documents rejected with incomplete raw evidence; ancillary JSON uses same strict guard'

        $env:ROTOR_BENCH_TEST_MODE = 'overflow'
        $limited = Invoke-ProcessWithTimeout $StubFile @('build') $root ([TimeSpan]::FromSeconds(3)) (Join-Path $root 'overflow.stdout') (Join-Path $root 'overflow.stderr') -CaptureLimit 1024
        Assert-True ($limited.Status -eq 'incomplete' -and $limited.CaptureOverflow -and $limited.StdoutBytes -eq 1024) 'overflow did not invalidate/retain bounded prefix'
        Assert-True ((Get-Item -LiteralPath (Join-Path $root 'overflow.stdout')).Length -eq 1024) 'capture exceeded its per-stream disk bound'
        $savedLimit = $script:MaxCaptureBytes
        try {
            $script:MaxCaptureBytes = 1024
            $script:EvidenceDir = Join-Path $root 'overflow-run'
            Assert-Throws { Invoke-RealBenchmark $Repository } 'capture overflow'
            $overflowRun = Read-StrictJson (Join-Path $EvidenceDir 'run.json')
            Assert-True ($overflowRun.status -eq 'incomplete' -and -not (Test-Path -LiteralPath (Join-Path $EvidenceDir 'windows-build-performance-manifest.json'))) 'overflow run published a completed manifest'
        } finally { $script:MaxCaptureBytes = $savedLimit }
        $writer = (Get-Item Function:Write-ProcessEvidence).ScriptBlock
        try {
            function Write-ProcessEvidence { param($Owner); throw 'injected tail retention failure' }
            $env:ROTOR_BENCH_TEST_MODE = 'closed-success'
            $env:ROTOR_BENCH_TEST_PID = Join-Path $root 'retention.pid'
            $env:ROTOR_BENCH_TEST_DONE = Join-Path $root 'retention.done'
            Assert-Throws { Invoke-ScoredBuild $StubFile $root ([TimeSpan]::FromSeconds(3)) (Join-Path $root 'retention') $journal @{ finding = 'C2/C5'; scored = $false } } 'retention failure'
            Assert-True ((Read-Journal $journal)[-1].status -eq 'incomplete') 'retention failure was completed'
            Assert-True (Test-Path -LiteralPath $env:ROTOR_BENCH_TEST_DONE) 'retention failure truncated successful required child persistence'
        } finally { Set-Item Function:Write-ProcessEvidence $writer }
        Write-Output 'PASS R2-C5: capture overflow bounded and incomplete; injected tail retention failure cannot complete'

        # C3: cancellation after process success, during output validation.
        $env:ROTOR_BENCH_TEST_MODE = ''
        $cancelRoot = Join-Path $root 'cancel-validation'
        [void][IO.Directory]::CreateDirectory($cancelRoot)
        $pipeline = [PowerShell]::Create()
        try {
            [void]$pipeline.AddScript({
                param($HarnessPath, $StubPath, $CaseRoot)
                . $HarnessPath
                function Get-TreeDigest {
                    param($TreeRoot, [switch]$OutputStateOnly)
                    [IO.File]::WriteAllText((Join-Path $CaseRoot 'hash-entered'), 'ready')
                    Start-Sleep -Seconds 8
                    return 'not reached'
                }
                [void](Invoke-ScoredBuild $StubPath $CaseRoot ([TimeSpan]::FromSeconds(3)) (Join-Path $CaseRoot 'invocation') (Join-Path $CaseRoot 'journal') @{ scored = $false })
            }).AddArgument($HarnessFile).AddArgument($StubFile).AddArgument($cancelRoot)
            [void]$pipeline.BeginInvoke()
            $ready = [Diagnostics.Stopwatch]::StartNew()
            while (-not (Test-Path -LiteralPath (Join-Path $cancelRoot 'hash-entered')) -and $ready.Elapsed.TotalSeconds -lt 5) { Start-Sleep -Milliseconds 10 }
            Assert-True (Test-Path -LiteralPath (Join-Path $cancelRoot 'hash-entered')) 'validation cancellation hook not reached'
            $pipeline.Stop()
            $last = (Read-Journal (Join-Path $cancelRoot 'journal'))[-1]
            Assert-True ($last.status -eq 'incomplete' -and -not $last.validation_completed -and $last.error) 'late cancellation falsely completed validation'
        } finally { $pipeline.Dispose() }
        Write-Output 'PASS R2-C3: cancellation during output hashing remains explicitly incomplete'

        $one = Join-Path $root 'one-file'
        $two = Join-Path $root 'two-files'
        foreach ($tree in @($one, $two)) { [void][IO.Directory]::CreateDirectory((Join-Path $tree 'out')) }
        [IO.File]::WriteAllBytes((Join-Path $one 'out/a.bin'), [Text.Encoding]::UTF8.GetBytes("x`0out/b.bin`0y"))
        [IO.File]::WriteAllText((Join-Path $two 'out/a.bin'), 'x')
        [IO.File]::WriteAllText((Join-Path $two 'out/b.bin'), 'y')
        Assert-True ((Get-TreeDigest $one) -cne (Get-TreeDigest $two)) 'split-file collision remains in canonical digest'
        Assert-True ((Get-TreeDigest $one -OutputStateOnly) -cne (Get-TreeDigest $two -OutputStateOnly)) 'split-file collision remains in output digest'
        Assert-True ((Get-TreeDigest $one) -match '^sha256-tree-v2:[0-9a-f]{64}$') 'digest format identity missing'
        Write-Output 'PASS R2-P03: split-file collision rejected in both canonical/output scopes; digest carries v2 format identity'

        # C4: fabricate only invocation durations/results, then exercise actual
        # identity/evaluator/cleanup/publication. No compiler performance score.
        function Invoke-ScoredBuild {
            param($Executable, $WorkCopy, $Timeout, $EvidencePath, $JournalPath, $Invocation, $ExtraArguments)
            if ($Invocation.phase -eq 'diagnostic') {
                [IO.File]::WriteAllText($ExtraArguments[1], '{"schemaVersion":2,"ok":true,"totalMs":1,"stageSemantics":"workMs","stages":{},"counts":{},"metadata":{}}')
                if ($cleanupRefusal) { [void](New-Item -ItemType Junction -Path (Join-Path $WorkCopy 'escape') -Target $sentinel) }
            }
            $duration = if ($Invocation.binary -eq 'baseline') { 1000 } elseif ($legacyFailure) { 1000 } else { 600 }
            return @{ DurationMS = $duration; ExitCode = 0; DiagnosticsDigest = 'synthetic'; OutputTreeDigest = 'synthetic' }
        }
        $sentinel = Join-Path $root 'sentinel'
        [void][IO.Directory]::CreateDirectory($sentinel)
        [IO.File]::WriteAllText((Join-Path $sentinel 'keep'), 'keep')
        $script:Fixture = Join-Path $root 'publication-fixture'
        [void][IO.Directory]::CreateDirectory($Fixture)
        [IO.File]::WriteAllText((Join-Path $Fixture 'input.ts'), 'synthetic')
        $script:ColdPairs = 10; $script:NoChangeRuns = 20; $script:SidecarTimeout = '3s'
        foreach ($case in @('cleanup-pass', 'cleanup-fail', 'legacy-fail')) {
            $cleanupRefusal = $case -ne 'legacy-fail'
            $legacyFailure = $case -ne 'cleanup-pass'
            $script:EvidenceDir = Join-Path $root $case
            Assert-Throws { Invoke-RealBenchmark $Repository } 'cleanup|perfcompare rejected'
            $run = Read-StrictJson (Join-Path $EvidenceDir 'run.json')
            $final = Join-Path $EvidenceDir 'windows-build-performance-manifest.json'
            if ($cleanupRefusal) {
                Assert-True ($run.status -eq 'incomplete' -and $run.error -and $run.cleanup_error -and -not (Test-Path -LiteralPath $final)) 'cleanup refusal published completion'
                Assert-True (Test-Path -LiteralPath "$final.pending") 'cleanup refusal lost pending evidence'
                Assert-True ($run.evaluation.Status -eq $(if ($legacyFailure) { 'FAIL' } else { 'PASS' })) 'cleanup failure lost actual evaluator result'
                Assert-True (Test-Path -LiteralPath (Join-Path $sentinel 'keep')) 'cleanup refused tree damaged external sentinel'
            } else {
                Assert-True ($run.status -eq 'evaluated' -and $run.evaluation.Status -eq 'FAIL' -and (Test-Path -LiteralPath $final)) 'real legacy FAIL was converted to incomplete or suppressed'
            }
        }
        Write-Output 'PASS R2-C4: cleanup refusal keeps pending/incomplete evidence for evaluator PASS/FAIL; successful cleanup preserves evaluated legacy FAIL'
    } finally {
        foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    }
}
