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
        Invoke-DiagnosticAdmissionRegressions $HarnessFile $StubFile $Repository $root
        Invoke-ResourceRegressions $StubFile $root
        Invoke-StablePathRegressions $HarnessFile $StubFile $Repository $root
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
                Assert-Throws { Invoke-ScoredBuild $StubFile $root ([TimeSpan]::FromSeconds(3)) $prefix $journal @{ finding = 'C2/P01'; scored = $false } } 'failed|JSON|timed out'
            }
            $childId = [int](Get-Content -LiteralPath $env:ROTOR_BENCH_TEST_PID -Raw)
            if ($mode -eq 'closed-nonzero') {
                Assert-True ([IO.File]::ReadAllText("$prefix.stderr.txt") -match "negative child ready: $childId\b") 'negative case did not acknowledge its live identifiable child'
                Assert-True ((Read-Journal $journal)[-1].result.ExitCode -eq 9) 'negative child probe failed before deliberate exit 9'
                Assert-True (-not (Test-Path -LiteralPath $env:ROTOR_BENCH_TEST_DONE)) 'negative child finished persistence before containment'
            }
            Assert-True ($null -eq (Get-Process -Id $childId -ErrorAction SilentlyContinue)) "$mode left a descendant alive"
            if ($mode -eq 'closed-timeout') {
                Assert-True (-not (Test-Path -LiteralPath $env:ROTOR_BENCH_TEST_DONE)) 'overdue child completed after timeout'
                Assert-True ((Read-Journal $journal)[-1].result.ExitCode -eq 0) 'timeout probe did not reach root success with a live private child'
            }
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

        # C6/P04: diagnostic messages are JSON strings, even when they look
        # like equivalent instants. Keep both independent timestamp examples.
        $messages = @(
            '2026-10-18T01:00:00+01:00', '2026-10-18T02:00:00+02:00', '2026-10-18T01:00:00+01:00',
            '2026-10-18T00:00:00Z', '2026-10-18T00:00:00.000Z', '2026-10-18T00:00:00Z',
            'ordinary first message', 'ordinary second message', 'ordinary first message'
        )
        $diagnostics = @()
        foreach ($message in $messages) {
            $prefix = Join-Path $root "string-$($diagnostics.Count)"
            $env:ROTOR_BENCH_TEST_JSON = ConvertTo-Json -Compress -Depth 8 -InputObject ([ordered]@{
                version = 'test'; files = 1; durationMs = 1; ok = $true
                diagnostics = @([ordered]@{ file = 'input.ts'; line = 1; col = 1; code = 'TEST'; severity = 'warning'; message = $message })
            })
            $result = Invoke-ScoredBuild $StubFile $root ([TimeSpan]::FromSeconds(3)) $prefix $journal @{ finding = 'C6/P04'; scored = $false }
            $parsed = Read-StrictJson "$prefix.stdout.json" -Kind public_result
            Assert-True ($parsed.diagnostics[0].message -is [string] -and $parsed.diagnostics[0].message -ceq $message) 'JSON conversion changed diagnostic string type/text'
            $diagnostics += $result.DiagnosticsDigest
        }
        foreach ($case in @(
            @{ name = 'offset-different'; a = 0; b = 1; equal = $false },
            @{ name = 'offset-same'; a = 0; b = 2; equal = $true },
            @{ name = 'fraction-different'; a = 3; b = 4; equal = $false },
            @{ name = 'fraction-same'; a = 3; b = 5; equal = $true },
            @{ name = 'ordinary-different'; a = 6; b = 7; equal = $false },
            @{ name = 'ordinary-same'; a = 6; b = 8; equal = $true }
        )) {
            Assert-True (($diagnostics[$case.a] -ceq $diagnostics[$case.b]) -eq $case.equal) "wrong diagnostic identity: $($case.name)"
            # Only observed digests enter this fabricated threshold fixture;
            # actual stub durations are never used as compiler scores.
            $manifest = Read-StrictJson (Join-Path (Split-Path $HarnessFile) '../perfcompare/testdata/pass.json')
            foreach ($record in $manifest.records) {
                $record.diagnostics_digest = $diagnostics[$(if ($record.binary -eq 'baseline') { $case.a } else { $case.b })]
            }
            $manifestPath = Join-Path $root "string-$($case.name).manifest.json"
            $verdictPath = Join-Path $root "string-$($case.name).verdict.json"
            $manifest | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $manifestPath
            $evaluation = Invoke-PerfCompare $Repository $manifestPath $verdictPath
            $verdict = Read-StrictJson $verdictPath -Kind verdict
            Assert-True (($evaluation.ExitCode -eq 0) -eq $case.equal) "real evaluator missed diagnostic identity: $($case.name)"
            $codes = @($verdict.reasons | ForEach-Object { $_.code })
            Assert-True (('diagnostics_digest_mismatch' -in $codes) -eq (-not $case.equal)) "wrong evaluator reason: $($case.name)"
        }
        Write-Output 'PASS R3-C6/P04: both timestamp spellings and ordinary-message differences reach real evaluator as diagnostic mismatches; same-string controls pass'

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
        Invoke-PublicationRegressions $HarnessFile $StubFile $Repository $root
    } finally {
        foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    }
}

function Invoke-DiagnosticAdmissionRegressions {
    param([string]$HarnessFile, [string]$StubFile, [string]$Repository, [string]$Artifacts)

    $root = Join-Path $Artifacts 'diagnostic-admission'
    [void][IO.Directory]::CreateDirectory($root)
    $failures = [Collections.Generic.List[string]]::new()
    $rows = [ordered]@{}
    $savedMode = $env:ROTOR_BENCH_TEST_MODE
    $savedJson = $env:ROTOR_BENCH_TEST_JSON
    function Observe-Admission {
        param([string]$Name, [string]$Members, [bool]$Accepted = $true)
        $raw = '{"version":"test","files":1,"durationMs":1,"ok":true,"diagnostics":[{"file":"input.ts","line":1,"col":1,"code":"TEST","severity":"warning",' + $Members + '}]}'
        $env:ROTOR_BENCH_TEST_JSON = $raw
        $prefix = Join-Path $root $Name
        $result = $null; $errorText = $null
        try { $result = Invoke-ScoredBuild $StubFile $root ([TimeSpan]::FromSeconds(4)) $prefix (Join-Path $root 'journal.ndjson') @{ scored = $false; test = $Name } }
        catch { $errorText = $_.Exception.Message }
        $entry = Get-Content -LiteralPath (Join-Path $root 'journal.ndjson') -Tail 1 | ConvertFrom-Json -AsHashtable
        $actual = $null -ne $result
        if ($actual -ne $Accepted) { $failures.Add("$Name accepted=$actual, expected=$Accepted, error=$errorText") }
        if ([IO.File]::ReadAllText("$prefix.stdout.json") -cne $raw) { $failures.Add("$Name lost raw evidence") }
        if (-not $actual -and ($entry.status -ne 'incomplete' -or $entry.validation_completed -or -not $entry.error)) { $failures.Add("$Name lost incomplete validation evidence") }
        $rows[$Name] = @{ accepted = $actual; result = $result; error = $errorText }
    }
    try {
        $env:ROTOR_BENCH_TEST_MODE = 'raw-json'
        foreach ($token in @('\ud800','\ud801','\udc00','\udc01','\ud800x','\ud800\ud801','\ude00\ud83d')) {
            Observe-Admission "invalid-$($rows.Count)" ('"message":"' + $token + '"') $false
        }
        Observe-Admission 'invalid-nested' '"message":"outer","extension":{"list":["\ud800"]}' $false
        Observe-Admission 'invalid-key' '"message":"outer","\ud800":1' $false
        Observe-Admission 'escaped-duplicate' '"message":"a","\u006dessage":"b"' $false
        Observe-Admission 'replacement-escaped' '"message":"\ufffd"'
        Observe-Admission 'replacement-literal' ('"message":"' + [char]0xfffd + '"')
        Observe-Admission 'emoji-escaped' '"message":"\ud83d\ude00"'
        Observe-Admission 'emoji-literal' ('"message":"' + [char]::ConvertFromUtf32(0x1f600) + '"')
        Observe-Admission 'composed' '"message":"\u00e9"'
        Observe-Admission 'decomposed' '"message":"e\u0301"'
        foreach ($depth in @(34,59,60,61)) {
            foreach ($leaf in @('alpha','beta','repeat')) {
                $extension = '{"message":"' + $(if ($leaf -eq 'repeat') { 'alpha' } else { $leaf }) + '"}'
                foreach ($index in 1..$depth) { $extension = '{"next":' + $extension + '}' }
                Observe-Admission "depth-$depth-$leaf" ('"message":"outer","extension":' + $extension) ($depth -le 60)
            }
        }
        $array = '"leaf"'
        foreach ($index in 1..61) { $array = '[' + $array + ']' }
        Observe-Admission 'array-at-limit' ('"message":"outer","extension":' + $array)
        Observe-Admission 'array-over-limit' ('"message":"outer","extension":[' + $array + ']') $false
        $numbers = @('1.00000000000000001','1.00000000000000002','1e500','1e501','1','1.0','1e0','9007199254740992','9007199254740993')
        for ($index = 0; $index -lt $numbers.Count; $index++) {
            Observe-Admission "number-$index" ('"message":"outer","extension":{"list":[null,true,false,{"number":' + $numbers[$index] + '}]}')
        }
        Observe-Admission 'object-a' '"message":"outer","extension":{"z":[null,true,false],"a":{"s":"雪","n":1e500}}'
        Observe-Admission 'object-b' '"message":"outer","extension":{"a":{"n":1e500,"s":"\u96ea"},"z":[null,true,false]}'
        Observe-Admission 'typed-string' '"message":"outer","extension":"true"'
        Observe-Admission 'typed-boolean' '"message":"outer","extension":true'
        $pairs = @(
            @('replacement-escaped','replacement-literal',$true), @('emoji-escaped','emoji-literal',$true),
            @('composed','decomposed',$false), @('object-a','object-b',$true), @('typed-string','typed-boolean',$false),
            @('number-0','number-1',$false), @('number-2','number-3',$false),
            @('number-4','number-5',$false), @('number-5','number-6',$false), @('number-7','number-8',$false)
        )
        foreach ($depth in @(34,59,60)) { $pairs += ,@("depth-$depth-alpha","depth-$depth-beta",$false); $pairs += ,@("depth-$depth-alpha","depth-$depth-repeat",$true) }
        $evaluations = @()
        foreach ($pair in $pairs) {
            $a = $rows[$pair[0]]; $b = $rows[$pair[1]]
            if (-not $a.accepted -or -not $b.accepted) { $failures.Add("pair not admitted: $($pair[0]), $($pair[1])"); continue }
            if (($a.result.DiagnosticsDigest -ceq $b.result.DiagnosticsDigest) -ne $pair[2]) { $failures.Add("digest identity: $($pair[0]), $($pair[1])") }
            $manifest = Read-StrictJson (Join-Path (Split-Path $HarnessFile) '../perfcompare/testdata/pass.json')
            foreach ($record in $manifest.records) { $record.diagnostics_digest = if ($record.binary -eq 'baseline') { $a.result.DiagnosticsDigest } else { $b.result.DiagnosticsDigest } }
            $name = $pair[0] + '--' + $pair[1]
            $path = Join-Path $root "$name.synthetic.json"
            $verdictPath = Join-Path $root "$name.verdict.json"
            $manifest | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $path
            $evaluation = Invoke-PerfCompare $Repository $path $verdictPath
            $verdict = Read-StrictJson $verdictPath -Kind verdict
            $codes = @($verdict.reasons | ForEach-Object code)
            if (($evaluation.Status -eq 'PASS') -ne $pair[2] -or ('diagnostics_digest_mismatch' -in $codes) -ne (-not $pair[2])) { $failures.Add("actual evaluator identity: $name") }
            $evaluations += @{ pair = $pair; evaluation = $evaluation; reasons = $codes }
        }
        @{ cases = $rows; evaluations = $evaluations; failures = @($failures) } | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $root 'results.json')
        if ($failures.Count) { throw ($failures -join '; ') }
        Write-Output 'PASS R4-C8/C9/P06/A01: accepted-depth, Unicode scalars, exact number tokens and JSON types reach real evaluator; rejected inputs preserve incomplete raw evidence'
    } finally { $env:ROTOR_BENCH_TEST_MODE = $savedMode; $env:ROTOR_BENCH_TEST_JSON = $savedJson }
}

function Invoke-ResourceRegressions {
    param([string]$StubFile, [string]$Artifacts)

    $root = Join-Path $Artifacts 'resources'
    [void][IO.Directory]::CreateDirectory($root)
    $savedMode = $env:ROTOR_BENCH_TEST_MODE
    $savedRoot = $env:ROTOR_BENCH_TEST_RESOURCE_ROOT
    $rows = [ordered]@{}
    try {
        foreach ($mode in @('leaf','cpu','child-cpu','io','child-io','staggered','overlap','timeout','nonzero')) {
            $case = Join-Path $root $mode
            [void][IO.Directory]::CreateDirectory($case)
            $env:ROTOR_BENCH_TEST_MODE = "resource-$mode"
            $env:ROTOR_BENCH_TEST_RESOURCE_ROOT = $case
            $deadline = if ($mode -eq 'timeout') { [TimeSpan]::FromSeconds(1) } else { [TimeSpan]::FromSeconds(12) }
            $result = Invoke-ProcessWithTimeout $StubFile @('build') $case $deadline (Join-Path $case 'stdout') (Join-Path $case 'stderr')
            $result | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $case 'result.json')
            $rows[$mode] = $result
            $resources = $result.ResourceAccounting
            $partial = $mode -in @('timeout','nonzero')
            $status = if ($partial -or $resources.OtherOwnedCpu.Status -ne 'available') { 'partial' } else { 'available' }
            Assert-True ($resources.Status -eq $status -and $resources.Completion -eq $(if ($partial) { 'partial' } else { 'complete' })) "resource completeness wrong: $mode"
            Assert-True ($resources.JobAccounting.Values.ActiveProcesses -eq 0 -and $resources.JobAccounting.Values.TotalProcesses -ge 1) 'resource snapshot preceded owned-job quiescence'
            Assert-True ($resources.RootCpu.Status -eq 'available' -and $resources.JobAccounting.Status -eq 'available') 'raw CPU components unavailable on supported control'
            if ($resources.OtherOwnedCpu.Status -ne 'available') {
                Assert-True ($resources.OtherOwnedCpu.Status -eq 'failed' -and $null -eq $resources.OtherOwnedCpu.Values -and $resources.OtherOwnedCpu.Error -like '*inconsistent*') 'CPU inconsistency lost its unmeasured cause'
                Assert-True ($resources.JobAccounting.Values.UserTicks100ns -lt $resources.RootCpu.Values.UserTicks100ns -or $resources.JobAccounting.Values.KernelTicks100ns -lt $resources.RootCpu.Values.KernelTicks100ns) 'CPU inconsistency was not supported by raw values'
            }
            if ($partial) { Assert-True ([bool]$resources.CompletionError) 'partial resources lost failure cause' }
            if ($mode -in @('cpu','child-cpu')) {
                $tag = if ($mode -eq 'cpu') { 'root' } else { 'child' }
                Assert-True ((Test-Path -LiteralPath (Join-Path $case "$tag-cpu")) -and (Test-Path -LiteralPath (Join-Path $case "$tag-done"))) 'CPU work did not complete'
                $cpu = if ($tag -eq 'root') { $resources.RootCpu } else { $resources.OtherOwnedCpu }
                if ($cpu.Status -eq 'available') {
                    Assert-True (($cpu.Values.UserTicks100ns + $cpu.Values.KernelTicks100ns) -gt 0) 'completed fixed CPU work was missing from retained counters'
                } else {
                    Assert-True (($resources.JobAccounting.Values.UserTicks100ns + $resources.JobAccounting.Values.KernelTicks100ns) -gt ($resources.RootCpu.Values.UserTicks100ns + $resources.RootCpu.Values.KernelTicks100ns)) 'child CPU work missing from raw job/root totals'
                }
            }
            if ($mode -in @('child-cpu','child-io')) {
                Assert-True (Test-Path -LiteralPath (Join-Path $case 'child-parent-exited')) 'child did not observe parent exit before resource work'
                $childId = [int][IO.File]::ReadAllText((Join-Path $case 'child-pid'))
                Assert-True ($null -eq (Get-Process -Id $childId -ErrorAction SilentlyContinue)) 'resource child still alive after measured completion'
                Assert-True ($resources.JobAccounting.Values.TotalProcesses -ge 2) 'exited child omitted from lifetime count'
            }
            if ($mode -in @('io','child-io','timeout','nonzero')) {
                $tag = if ($mode -eq 'child-io') { 'child' } else { 'root' }
                $file = Join-Path $case "$tag-io.bin"
                Assert-True ((Get-Item -LiteralPath $file).Length -eq 16MB -and [IO.File]::ReadAllText((Join-Path $case "$tag-io-verified")) -eq (Get-FileHash $file).Hash) 'resource I/O artifacts not verified'
                Assert-True ($resources.JobAccounting.Values.ReadBytes -ge 16MB -and $resources.JobAccounting.Values.WriteBytes -ge 16MB) 'completed OS I/O missing from job accounting'
            }
            if ($mode -in @('staggered','overlap')) {
                foreach ($tag in @('a','b')) { Assert-True (Test-Path -LiteralPath (Join-Path $case "$tag-done")) 'allocation control did not complete' }
                Assert-True ($resources.JobMemory.Values.PeakProcessMemoryBytes -ge 128MB) 'touched allocation missing from retained process peak'
            }
        }
        Assert-True ($rows['overlap'].ResourceAccounting.JobMemory.Values.PeakJobMemoryBytes -gt $rows['staggered'].ResourceAccounting.JobMemory.Values.PeakJobMemoryBytes) 'job memory peak did not distinguish overlap from staggered allocations'
        Assert-True ($rows['overlap'].ResourceAccounting.JobMemory.Values.PeakJobMemoryBytes -gt $rows['overlap'].ResourceAccounting.JobMemory.Values.PeakProcessMemoryBytes) 'job peak was substituted with individual process peak'
        $rows | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $root 'controls.json')
        Write-Output 'PASS resources: root/exited-private-child CPU and verified OS I/O, lifetime counts, staggered/overlapping touched peaks, timeout/nonzero partial accounting'

        $owner = [RotorBench.Invocation]::new((Join-Path $root 'unstarted.stdout'), (Join-Path $root 'unstarted.stderr'), 1024)
        try { $missing = $owner.ReadResources($true, 'unstarted control'); Assert-True ($missing.Status -eq 'unavailable' -and $null -eq $missing.JobAccounting.Values -and $missing.JobAccounting.Error) 'unavailable counters became successful zero' }
        finally { $owner.Dispose() }
        $env:ROTOR_BENCH_TEST_MODE = 'resource-leaf'
        $env:ROTOR_BENCH_TEST_RESOURCE_ROOT = Join-Path $root 'fault-leaf'
        $owner = $null
        try {
            [void](Invoke-ProcessWithTimeout $StubFile @('build') $root ([TimeSpan]::FromSeconds(4)) (Join-Path $root 'fault.stdout') (Join-Path $root 'fault.stderr') -Ownership ([ref]$owner))
            $field = $owner.GetType().GetField('job', [Reflection.BindingFlags]'NonPublic,Instance')
            $heldJob = $field.GetValue($owner)
            try {
                # Read fault only after quiescence; restore the real owned handle
                # before disposal. No test process is allowed to escape its job.
                $field.SetValue($owner, [IntPtr]::new(1))
                $failed = $owner.ReadResources($false, $null)
                Assert-True ($failed.Status -eq 'partial' -and $failed.JobAccounting.Status -eq 'failed' -and $failed.JobMemory.Status -eq 'failed' -and $failed.RootCpu.Status -eq 'available' -and $null -eq $failed.JobAccounting.Values -and $failed.JobAccounting.Error) 'native getter failure became zero or erased available components'
            } finally { $field.SetValue($owner, $heldJob) }
        } finally { if ($null -ne $owner) { $owner.Dispose() } }
        $jobValues = [Collections.Generic.Dictionary[string,object]]::new()
        $jobValues['UserTicks100ns'] = [long]1; $jobValues['KernelTicks100ns'] = [long]2
        $parentValues = [Collections.Generic.Dictionary[string,object]]::new()
        $parentValues['UserTicks100ns'] = [long]2; $parentValues['KernelTicks100ns'] = [long]2
        $job = [RotorBench.ResourceMetric]::Available($jobValues)
        $parent = [RotorBench.ResourceMetric]::Available($parentValues)
        $inconsistent = [RotorBench.ResourceSnapshot]::SubtractOtherCpu($job, $parent)
        Assert-True ($inconsistent.Status -eq 'failed' -and $null -eq $inconsistent.Values -and $inconsistent.Error -like '*not clamped*') 'inconsistent subtraction was silently clamped'
        $fake = [pscustomobject]@{}
        $fake | Add-Member ScriptMethod ReadResources { param($Partial, $Failure); throw 'injected resource getter failure' }
        $originalReader = (Get-Item Function:Read-InvocationResources).ScriptBlock
        try {
            function Read-InvocationResources { param($Owner, [bool]$Partial, [string]$Failure); return & $originalReader $fake $Partial $Failure }
            $env:ROTOR_BENCH_TEST_MODE = ''
            $good = Invoke-ScoredBuild $StubFile $root ([TimeSpan]::FromSeconds(4)) (Join-Path $root 'query-failure-good') (Join-Path $root 'fault.ndjson') @{ scored = $false }
            Assert-True ($good.Status -eq 'completed' -and $good.ResourceAccounting.Status -eq 'failed' -and $null -eq $good.ResourceAccounting.JobAccounting.Values) 'resource failure invalidated valid wall measurement or became zero'
            $env:ROTOR_BENCH_TEST_MODE = 'nonzero'
            Assert-Throws { Invoke-ScoredBuild $StubFile $root ([TimeSpan]::FromSeconds(4)) (Join-Path $root 'query-failure-bad') (Join-Path $root 'fault.ndjson') @{ scored = $false } } 'build failed.*exit_code=9'
            $last = (Read-Journal (Join-Path $root 'fault.ndjson'))[-1]
            Assert-True ($last.error -like '*build failed*' -and $last.result.ResourceAccounting.Status -eq 'failed') 'resource failure masked original process error'
        } finally { Set-Item Function:Read-InvocationResources $originalReader }
        $unsupported = [RotorBench.ResourceSnapshot]::Missing('unsupported', 'unsupported-platform representation control', $false)
        Assert-True ($unsupported.Status -eq 'unsupported' -and $null -eq $unsupported.JobAccounting.Values -and $unsupported.JobAccounting.Error) 'portable branch queried native counters or became zero'
        @{ unavailable = $missing; native_failure = $failed; inconsistent = $inconsistent; unsupported_branch = $unsupported; valid_wall_query_failure = $good.ResourceAccounting } | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $root 'faults.json')

        $env:ROTOR_BENCH_TEST_MODE = ''
        try {
            function Read-InvocationResources { param($Owner, [bool]$Partial, [string]$Failure); Start-Sleep -Milliseconds 700; return & $originalReader $Owner $Partial $Failure }
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $delayed = Invoke-ProcessWithTimeout $StubFile @('build') $root ([TimeSpan]::FromSeconds(4)) (Join-Path $root 'delayed.stdout') (Join-Path $root 'delayed.stderr')
            $timer.Stop()
            @{ result = $delayed; total_ms = $timer.Elapsed.TotalMilliseconds } | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath (Join-Path $root 'post-wall-delay.json')
            Assert-True (($timer.Elapsed.TotalMilliseconds - $delayed.ElapsedMS) -ge 650 -and $delayed.ResourceAccounting.JobAccounting.Status -eq 'available') 'resource getter delay contaminated wall duration'
        } finally { Set-Item Function:Read-InvocationResources $originalReader }
        Write-Output 'PASS resources: unavailable/native-failed/unsupported states retain null values and causes; inconsistent CPU fails; collector failure preserves original outcome; getter delay excluded from wall endpoint'
    } finally { $env:ROTOR_BENCH_TEST_MODE = $savedMode; $env:ROTOR_BENCH_TEST_RESOURCE_ROOT = $savedRoot }
}

function Invoke-StablePathRegressions {
    param([string]$HarnessFile, [string]$StubFile, [string]$Repository, [string]$Artifacts)

    $root = Join-Path $Artifacts 'stable-path'
    [void][IO.Directory]::CreateDirectory($root)
    $fixture = Join-Path $root 'fixture'
    [void][IO.Directory]::CreateDirectory((Join-Path $fixture 'src/empty/nested'))
    [IO.File]::WriteAllText((Join-Path $fixture 'src/input.ts'), 'synthetic stable path input')
    foreach ($record in @(Get-ProjectMetadata $fixture) | Sort-Object { $_.path.Length } -Descending) {
        $path = Join-Path $fixture $record.path
        $time = [DateTime]::new(638500000000000000L + $record.path.Length, [DateTimeKind]::Utc)
        if ($record.kind -eq 'directory') { [IO.Directory]::SetLastWriteTimeUtc($path, $time) } else { [IO.File]::SetLastWriteTimeUtc($path, $time) }
    }
    $canonical = Get-ProjectMetadataDigest @(Get-ProjectMetadata $fixture)
    $bin = Join-Path $root 'bin'
    [void][IO.Directory]::CreateDirectory($bin)
    foreach ($arm in @('baseline','candidate')) { Copy-Item -LiteralPath $StubFile -Destination (Join-Path $bin "$arm.exe") }
    function New-Case([string]$Name) {
        $case = Join-Path $root $Name
        [void][IO.Directory]::CreateDirectory($case)
        $work = Join-Path $case ('work-' + [Guid]::NewGuid().ToString('N'))
        [void][IO.Directory]::CreateDirectory($work)
        return New-StableProjectState $fixture $work $case
    }
    function Output-Metadata([string]$Path) { return Get-ProjectMetadataDigest @(Get-ProjectMetadata (Join-Path $Path 'out')) }
    $savedMode = $env:ROTOR_BENCH_TEST_MODE
    try {
        $env:ROTOR_BENCH_TEST_MODE = 'stable-state'
        $default = Join-Path $root 'default'
        [void][IO.Directory]::CreateDirectory($default)
        $digests = @()
        foreach ($arm in @('baseline','candidate')) {
            $copy = Join-Path $default $arm
            Copy-Item -LiteralPath $fixture -Destination $copy -Recurse
            $result = Invoke-ScoredBuild (Join-Path $bin "$arm.exe") $copy ([TimeSpan]::FromSeconds(3)) (Join-Path $default $arm) (Join-Path $default 'journal') @{ scored = $false }
            $digests += $result.OutputTreeDigest
        }
        Assert-True ($digests[0] -cne $digests[1]) 'default different-path control unexpectedly normalized emitted paths'
        $state = New-Case 'alternating'
        $reference = $null; $prior = @{}
        foreach ($phase in @('cold','no_change','no_change_2')) {
            $order = if ($phase -eq 'no_change') { @('candidate','baseline') } else { @('baseline','candidate') }
            foreach ($arm in $order) {
                $result = Invoke-StableProjectBuild $state $arm (Join-Path $bin "$arm.exe") ([TimeSpan]::FromSeconds(3)) (Join-Path $state.evidence_root "$phase-$arm") (Join-Path $state.evidence_root 'journal') @{ scored = $false; phase = $phase } -Cold:($phase -eq 'cold')
                if ($null -eq $reference) { $reference = $result.OutputTreeDigest }
                Assert-True ($result.OutputTreeDigest -ceq $reference) 'stable path failed exact literal output parity'
                $current = Output-Metadata $state.arms[$arm]
                if ($prior.ContainsKey($arm)) { Assert-True ($current -ceq $prior[$arm]) 'no-change output bytes/mtimes changed' }
                $prior[$arm] = $current
                Assert-True (-not (Test-Path -LiteralPath $state.active_path) -and $state.transition -eq 'inactive') 'successful arm did not return to its own inactive slot'
                Assert-True ([IO.Directory]::GetLastWriteTimeUtc((Join-Path $state.arms[$arm] 'src/empty/nested')).Ticks -eq 638500000000000016L) 'empty-directory input timestamp changed'
            }
        }
        foreach ($arm in @('baseline','candidate')) {
            Assert-True ([IO.File]::ReadAllText((Join-Path $state.arms[$arm] '.rotor/count')) -eq '3') 'arm cache was not independently retained'
            Assert-True ([IO.File]::ReadAllText((Join-Path $state.arms[$arm] '.rotor/arm')) -eq "$arm.exe") 'arm cache identity changed'
        }
        Assert-True (@(Read-Journal (Join-Path $state.evidence_root 'journal') | Where-Object event -eq 'started' | Where-Object working_directory -ne $state.active_path).Count -eq 0) 'compiler launched in an inactive path'
        Assert-True ((Get-ProjectMetadataDigest @(Get-ProjectMetadata $fixture)) -ceq $canonical) 'canonical file/directory metadata changed'
        Write-Output 'PASS stable path: default path mismatch retained; literal maps/manifests match at active path; AB/BA independent caches, no-change mtimes and input topology/times preserved'

        foreach ($fault in @('occupied-active','missing-source','occupied-inactive','source-reparse','destination-reparse','boundary','second-owner')) {
            $state = New-Case $fault
            $candidateBefore = Get-ProjectMetadataDigest @(Get-ProjectMetadata $state.arms.candidate)
            switch ($fault) {
                'occupied-active' { [void][IO.Directory]::CreateDirectory($state.active_path) }
                'missing-source' { [IO.Directory]::Move($state.arms.baseline, (Join-Path $state.work_root 'retained-missing-source')) }
                'occupied-inactive' { Move-StableProjectArm $state baseline; [void][IO.Directory]::CreateDirectory($state.arms.baseline) }
                'source-reparse' { [void](New-Item -ItemType Junction -Path (Join-Path $state.arms.baseline 'link') -Target $fixture) }
                'destination-reparse' { [void](New-Item -ItemType Junction -Path $state.active_path -Target $fixture) }
                'boundary' { $state.arms.baseline = Join-Path $root 'outside-slot' }
                'second-owner' { Move-StableProjectArm $state candidate }
            }
            Assert-Throws { Move-StableProjectArm $state baseline -Deactivate:($fault -eq 'occupied-inactive') } 'stable|reparse'
            Assert-True ($state.preserve_work -and $state.error -and $state.transition -eq 'incomplete') "move refusal lost pending state: $fault"
            if ($fault -ne 'boundary') { Assert-True ((Read-StrictJson $state.state_file).transition -eq 'incomplete') "move refusal was not retained on disk: $fault" }
            if ($fault -ne 'second-owner') { Assert-True ((Get-ProjectMetadataDigest @(Get-ProjectMetadata $state.arms.candidate)) -ceq $candidateBefore) "move refusal changed other arm: $fault" }
            if ($fault -eq 'occupied-inactive') { Assert-True ((Test-Path -LiteralPath $state.active_path) -and (Test-Path -LiteralPath $state.arms.baseline)) 'deactivation overwrote an occupied inactive slot' }
        }
        Assert-True ((Get-ProjectMetadataDigest @(Get-ProjectMetadata $fixture)) -ceq $canonical) 'path refusal damaged canonical reparse target'
        Write-Output 'PASS stable path: missing/occupied source/destination, reparses, exact-slot boundary and second owner rejected without overwriting other state'

        foreach ($mode in @('closed-private','nonzero','invalid','closed-timeout','mutate-fixture','mutate-other-arm','mutate-canonical')) {
            $state = New-Case $mode
            $env:ROTOR_BENCH_TEST_MODE = if ($mode -in @('mutate-other-arm','mutate-canonical')) { 'mutate-fixture' } else { $mode }
            $env:ROTOR_BENCH_TEST_PID = Join-Path $state.evidence_root 'child.pid'
            $env:ROTOR_BENCH_TEST_DONE = Join-Path $state.active_path 'out/child.done'
            $env:ROTOR_BENCH_TEST_MUTATE = Join-Path $state.active_path 'src/input.ts'
            if ($mode -eq 'mutate-other-arm') { $env:ROTOR_BENCH_TEST_MUTATE = Join-Path $state.arms.candidate 'src/input.ts' }
            if ($mode -eq 'mutate-canonical') { $env:ROTOR_BENCH_TEST_MUTATE = Join-Path $fixture 'src/input.ts' }
            $inputBytes = [IO.File]::ReadAllBytes((Join-Path $fixture 'src/input.ts')); $inputTime = [IO.File]::GetLastWriteTimeUtc((Join-Path $fixture 'src/input.ts'))
            $prefix = Join-Path $state.evidence_root 'invocation'
            $action = { Invoke-StableProjectBuild $state baseline (Join-Path $bin 'baseline.exe') ([TimeSpan]::FromMilliseconds(800)) $prefix (Join-Path $state.evidence_root 'journal') @{ scored = $false } }
            if ($mode -eq 'closed-private') {
                $result = & $action
                Assert-True ($result.DurationMS -ge 320 -and (Test-Path -LiteralPath (Join-Path $state.arms.baseline 'out/child.done'))) 'private child persistence was truncated or moved before quiescence'
            } else {
                $pattern = switch ($mode) { 'nonzero' { 'exit_code=9' }; 'invalid' { 'JSON' }; 'closed-timeout' { 'timed out' }; 'mutate-other-arm' { 'other arm' }; 'mutate-canonical' { 'canonical input' }; default { 'protected input' } }
                Assert-Throws $action $pattern
                $snapshot = Read-StrictJson $state.state_file
                Assert-True ($snapshot.active_owner -eq 'baseline' -and $snapshot.preserve_work -and $snapshot.error -match $pattern -and $snapshot.locations.active_exists) "failure lost original error/active ownership: $mode"
                Assert-True (-not (Test-Path -LiteralPath $state.arms.baseline) -and (Test-Path -LiteralPath $state.arms.candidate)) 'failure moved or replaced an arm'
                if ($mode -eq 'closed-timeout') { Assert-True ($null -eq (Get-Process -Id ([int][IO.File]::ReadAllText($env:ROTOR_BENCH_TEST_PID)) -ErrorAction SilentlyContinue)) 'overdue private child survived' }
                if ($mode -eq 'mutate-canonical') { [IO.File]::WriteAllBytes((Join-Path $fixture 'src/input.ts'), $inputBytes); [IO.File]::SetLastWriteTimeUtc((Join-Path $fixture 'src/input.ts'), $inputTime) }
            }
        }
        Write-Output 'PASS stable path: successful private child finishes persistence before deactivation; nonzero/invalid/timeout/input mutation retain original errors and truthful active state'

        foreach ($stage in @('activating','deactivating','compiler','main-failure','main-retention-failure','main-invalid-count','main-legacy-fail')) {
            $case = Join-Path $root "host-$stage"
            [void][IO.Directory]::CreateDirectory($case)
            $pipeline = [PowerShell]::Create()
            try {
                [void]$pipeline.AddScript({
                    param($HarnessPath, $BinaryRoot, $FixturePath, $RepositoryRoot, $CaseRoot, $Stage)
                    . $HarnessPath
                    $env:ROTOR_BENCH_TEST_MODE = if ($Stage -eq 'compiler') { 'hang' } elseif ($Stage -in @('main-failure','main-retention-failure')) { 'nonzero' } else { 'stable-state' }
                    $env:ROTOR_BENCH_TEST_PID = Join-Path $CaseRoot 'child.pid'
                    if ($Stage -like 'main-*') {
                        $script:StableProjectPath = $true; $script:BaselineExe = Join-Path $BinaryRoot 'baseline.exe'; $script:CandidateExe = Join-Path $BinaryRoot 'candidate.exe'
                        $script:Fixture = $FixturePath; $script:EvidenceDir = Join-Path $CaseRoot 'evidence'; $script:SidecarTimeout = '3s'
                        $script:ColdPairs = if ($Stage -eq 'main-legacy-fail') { 10 } else { 1 }; $script:NoChangeRuns = if ($Stage -eq 'main-legacy-fail') { 20 } else { 1 }
                        if ($Stage -eq 'main-retention-failure') {
                            $originalStatus = (Get-Item Function:Write-RunStatus).ScriptBlock
                            function Write-RunStatus {
                                param($Path, $Run)
                                & $originalStatus $Path $Run
                                if ($Run.Contains('status') -and $Run.status -eq 'incomplete' -and $Run.error -like '*exit_code=9*') { throw 'injected secondary run status failure' }
                            }
                        }
                        if ($Stage -eq 'main-legacy-fail') {
                            $originalScorer = (Get-Item Function:Invoke-ScoredBuild).ScriptBlock
                            function Invoke-ScoredBuild { param($Executable, $WorkCopy, $Timeout, $EvidencePath, $JournalPath, $Invocation, $ExtraArguments); $result = & $originalScorer $Executable $WorkCopy $Timeout $EvidencePath $JournalPath $Invocation $ExtraArguments; $result.DurationMS = 1000; return $result }
                        }
                        try { Invoke-RealBenchmark $RepositoryRoot } catch { [IO.File]::WriteAllText((Join-Path $CaseRoot 'caught'), $_.Exception.Message); [IO.File]::WriteAllText((Join-Path $CaseRoot 'status-retention-error'), [string]$_.Exception.Data['status_retention_error']) }
                    } else {
                        $work = Join-Path $CaseRoot ('work-' + [Guid]::NewGuid().ToString('N'))
                        [void][IO.Directory]::CreateDirectory($work)
                        $state = New-StableProjectState $FixturePath $work $CaseRoot
                        $originalSave = (Get-Item Function:Save-StableProjectState).ScriptBlock
                        function Save-StableProjectState {
                            param($State)
                            & $originalSave $State
                            if ($State.transition -eq $Stage) { [IO.File]::WriteAllText((Join-Path $CaseRoot 'ready'), 'ready'); Start-Sleep -Seconds 8 }
                        }
                        [void](Invoke-StableProjectBuild $state baseline (Join-Path $BinaryRoot 'baseline.exe') ([TimeSpan]::FromSeconds(10)) (Join-Path $CaseRoot 'invocation') (Join-Path $CaseRoot 'journal') @{ scored = $false })
                    }
                }).AddArgument($HarnessFile).AddArgument($bin).AddArgument($fixture).AddArgument($Repository).AddArgument($case).AddArgument($stage)
                $pending = $pipeline.BeginInvoke()
                if ($stage -like 'main-*') { Assert-True ($pending.AsyncWaitHandle.WaitOne(30000)) "main stable probe timed out: $stage" }
                else {
                    $signal = Join-Path $case $(if ($stage -eq 'compiler') { 'child.pid' } else { 'ready' })
                    $timer = [Diagnostics.Stopwatch]::StartNew()
                    while (-not (Test-Path -LiteralPath $signal) -and -not $pending.IsCompleted -and $timer.Elapsed.TotalSeconds -lt 8) { Start-Sleep -Milliseconds 10 }
                    Assert-True (Test-Path -LiteralPath $signal) "stable cancellation boundary not reached: $stage"
                    $pipeline.Stop()
                }
                if ($stage -like 'main-*') {
                    Assert-True ($pipeline.Streams.Error.Count -eq 0) "main stable host failed internally: $($pipeline.Streams.Error)"
                    $run = Read-StrictJson (Join-Path $case 'evidence/run.json')
                    if ($stage -eq 'main-legacy-fail') {
                        Assert-True ($run.status -eq 'evaluated' -and $run.evaluation.Status -eq 'FAIL' -and $run.project_state.cleanup_completed -and (Test-Path -LiteralPath (Join-Path $case 'evidence/windows-build-performance-manifest.json'))) 'stable mode lost legitimate legacy FAIL or cleanup/publication'
                    } else {
                        Assert-True ($run.status -eq 'incomplete' -and $run.error -and $run.project_state.preserve_work -and (Test-Path -LiteralPath $run.project_state.work_root) -and -not (Test-Path -LiteralPath (Join-Path $case 'evidence/windows-build-performance-manifest.json'))) 'stable main failure lost incomplete work/error'
                        if ($stage -in @('main-failure','main-retention-failure')) {
                            Assert-True ([IO.File]::ReadAllText((Join-Path $case 'caught')) -like '*exit_code=9*') 'stable cleanup/status retention masked original process failure'
                            if ($stage -eq 'main-retention-failure') { Assert-True ([IO.File]::ReadAllText((Join-Path $case 'status-retention-error')) -like '*secondary run status failure*') 'secondary status failure context was dropped' }
                        }
                        else { Assert-True ($run.evaluation.Status -eq 'FAIL' -and (Test-Path -LiteralPath (Join-Path $case 'evidence/windows-build-performance-manifest.json.pending'))) 'insufficient sample rejection lost pending/evaluator evidence' }
                    }
                } else {
                    $state = Read-StrictJson (Join-Path $case 'project-state.json')
                    Assert-True ($state.preserve_work -and $state.transition -eq 'incomplete' -and $state.error -like '*interrupt*') 'cancellation lost explicit pending/error state'
                    Assert-True (Test-Path -LiteralPath $state.arms.candidate) 'cancellation changed other arm'
                    if ($stage -eq 'activating') { Assert-True (-not $state.active_owner -and (Test-Path -LiteralPath $state.arms.baseline)) 'pre-move cancellation lost inactive owner' }
                    else { Assert-True ($state.active_owner -eq 'baseline' -and (Test-Path -LiteralPath $state.active_path)) 'cancellation moved active arm' }
                    if ($stage -eq 'compiler') { Assert-True ($null -eq (Get-Process -Id ([int][IO.File]::ReadAllText((Join-Path $case 'child.pid'))) -ErrorAction SilentlyContinue)) 'cancelled owned child survived' }
                }
            } finally { if ($pipeline.InvocationStateInfo.State -eq 'Running') { $pipeline.Stop() }; $pipeline.Dispose() }
        }
        Write-Output 'PASS stable path: activation/deactivation/compiler cancellation preserve ownership; main failures retain original errors; invalid samples stay pending; actual legacy FAIL remains evaluated with fabricated equal durations'
    } finally { $env:ROTOR_BENCH_TEST_MODE = $savedMode }
}

function Invoke-PublicationRegressions {
    param([string]$HarnessFile, [string]$StubFile, [string]$Repository, [string]$Artifacts)

    $binaries = Join-Path $Artifacts 'publication-bin'
    [void][IO.Directory]::CreateDirectory($binaries)
    Copy-Item -LiteralPath $StubFile -Destination (Join-Path $binaries 'baseline.exe')
    Copy-Item -LiteralPath $StubFile -Destination (Join-Path $binaries 'candidate.exe')
    foreach ($stage in @('sharing-violation', 'sharing-legacy-fail', 'cancel-publication', 'cancel-legacy-fail', 'status-failure', 'rollback-failure', 'secondary-status-failure')) {
        $caseRoot = Join-Path $Artifacts $stage
        [void][IO.Directory]::CreateDirectory($caseRoot)
        $pipeline = [PowerShell]::Create()
        try {
            [void]$pipeline.AddScript({
                param($HarnessPath, $BinaryRoot, $RepositoryRoot, $CaseRoot, $Stage)
                . $HarnessPath
                $script:BaselineExe = Join-Path $BinaryRoot 'baseline.exe'
                $script:CandidateExe = Join-Path $BinaryRoot 'candidate.exe'
                $script:Fixture = Join-Path $CaseRoot 'fixture'
                [void][IO.Directory]::CreateDirectory($Fixture)
                [IO.File]::WriteAllText((Join-Path $Fixture 'input.ts'), 'synthetic')
                $script:EvidenceDir = Join-Path $CaseRoot 'evidence'
                $script:ColdPairs = 10; $script:NoChangeRuns = 20; $script:SidecarTimeout = '3s'
                $script:FixtureIdentity = 'FABRICATED publication probe; no compiler score'
                function Get-WindowsMachineMetadata {
                    param($TimeoutValue, $Environment)
                    return @{ os = 'synthetic'; version = '1'; cpu = 'synthetic'; ram_bytes = 1; storage = 'synthetic'; power = 'synthetic'; run_order = @('AB', 'BA'); sidecar_timeout = $TimeoutValue; environment = $Environment }
                }
                function Invoke-ScoredBuild {
                    param($Executable, $WorkCopy, $Timeout, $EvidencePath, $JournalPath, $Invocation, $ExtraArguments)
                    if ($Invocation.phase -eq 'diagnostic') { [IO.File]::WriteAllText($ExtraArguments[1], '{"schemaVersion":2,"ok":true,"totalMs":1,"stageSemantics":"workMs","stages":{},"counts":{},"metadata":{}}') }
                    return @{ DurationMS = $(if ($Invocation.binary -eq 'baseline' -or $Stage -like '*legacy-fail') { 1000 } else { 600 }); ExitCode = 0; DiagnosticsDigest = 'synthetic'; OutputTreeDigest = 'synthetic' }
                }
                $originalStatus = (Get-Item Function:Write-RunStatus).ScriptBlock
                $script:heldFile = $null
                function Write-RunStatus {
                    param($Path, $Run)
                    & $originalStatus $Path $Run
                    if ($Run.status -eq 'evaluated') {
                        $final = Join-Path $script:EvidenceDir 'windows-build-performance-manifest.json'
                        if ($Stage -like 'cancel-*') {
                            [IO.File]::WriteAllText((Join-Path $CaseRoot 'ready'), 'ready')
                            Start-Sleep -Seconds 8
                        } elseif ($Stage -eq 'rollback-failure') {
                            [IO.File]::Move("$final.pending", $final)
                            $script:heldFile = [IO.File]::Open($final, 'Open', 'Read', 'Read')
                            throw 'injected original publication failure before rollback'
                        } elseif ($Stage -in @('status-failure', 'secondary-status-failure')) {
                            throw 'injected original publication status failure'
                        } else {
                            $script:heldFile = [IO.File]::Open("$final.pending", 'Open', 'Read', 'Read')
                        }
                    } elseif ($Stage -eq 'secondary-status-failure' -and $Run.error -like '*manifest publication failed*') {
                        throw 'injected secondary status failure after retention'
                    }
                }
                try { Invoke-RealBenchmark $RepositoryRoot }
                catch {
                    $details = @{ rollback_error = $_.Exception.Data['rollback_error']; status_retention_error = $_.Exception.Data['status_retention_error'] }
                    @{ message = $_.Exception.Message; data = $details } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $CaseRoot 'caught.json')
                } finally { if ($null -ne $script:heldFile) { $script:heldFile.Dispose() } }
            }).AddArgument($HarnessFile).AddArgument($binaries).AddArgument($Repository).AddArgument($caseRoot).AddArgument($stage)
            $pending = $pipeline.BeginInvoke()
            if ($stage -like 'cancel-*') {
                $timer = [Diagnostics.Stopwatch]::StartNew()
                while (-not (Test-Path -LiteralPath (Join-Path $caseRoot 'ready')) -and -not $pending.IsCompleted -and $timer.Elapsed.TotalSeconds -lt 15) { Start-Sleep -Milliseconds 10 }
                # Enumerating a live PSDataCollection in the assertion's
                # message would wait for completion before Stop can run.
                Assert-True (Test-Path -LiteralPath (Join-Path $caseRoot 'ready')) 'publication cancellation boundary not reached'
                $pipeline.Stop()
            } else {
                Assert-True ($pending.AsyncWaitHandle.WaitOne(15000)) "publication probe timed out: $stage"
            }
            Assert-True ($pipeline.Streams.Error.Count -eq 0) "publication probe failed internally: $($pipeline.Streams.Error)"
            $evidence = Join-Path $caseRoot 'evidence'
            $run = Read-StrictJson (Join-Path $evidence 'run.json')
            $final = Join-Path $evidence 'windows-build-performance-manifest.json'
            $expectedVerdict = if ($stage -like '*legacy-fail') { 'FAIL' } else { 'PASS' }
            Assert-True ($run.status -eq 'incomplete' -and $run.error -and $run.evaluation.Status -eq $expectedVerdict) "publication lost incomplete error/evaluator evidence: $stage"
            if ($expectedVerdict -eq 'FAIL') { Assert-True ($run.error -like '*perfcompare rejected benchmark evidence*') 'publication lost legitimate evaluator FAIL context' }
            if ($stage -eq 'rollback-failure') {
                Assert-True ((Test-Path -LiteralPath $final) -and $run.error -like '*manifest rollback failed*') 'rollback failure was not retained with incomplete status'
            } else {
                Assert-True (-not (Test-Path -LiteralPath $final) -and (Test-Path -LiteralPath "$final.pending")) "publication lost pending gate: $stage"
            }
            if ($stage -like 'cancel-*') {
                Assert-True ($run.error -like '*manifest publication interrupted*') 'publication cancellation lost interruption fallback'
            } else {
                $caught = Read-StrictJson (Join-Path $caseRoot 'caught.json')
                Assert-True ($run.error.Contains($caught.message)) 'retained status lost original publication exception'
                if ($stage -like 'sharing-*') { Assert-True ($caught.message -match 'Move.*used by another process') 'sharing violation was masked' }
                if ($stage -like '*status-failure') { Assert-True ($caught.message -eq 'injected original publication status failure') 'secondary status failure masked original' }
                if ($stage -eq 'secondary-status-failure') { Assert-True ($caught.data.status_retention_error -like '*secondary status failure*') 'secondary retention error context missing' }
                if ($stage -eq 'rollback-failure') { Assert-True ($caught.data.rollback_error -and $caught.message -eq 'injected original publication failure before rollback') 'rollback masked original failure' }
            }
        } finally {
            if ($pipeline.InvocationStateInfo.State -eq 'Running') { $pipeline.Stop() }
            $pipeline.Dispose()
        }
    }
    Write-Output 'PASS R3-C7: sharing violation, publication cancellation, status/rollback failures retain original context and incomplete gate; secondary errors do not mask original'
}
