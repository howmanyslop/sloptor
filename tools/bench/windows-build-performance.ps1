[CmdletBinding()]
param(
    [string]$BaselineExe,
    [string]$CandidateExe,
    [string]$Fixture,
    [int]$ColdPairs,
    [int]$NoChangeRuns,
    [string]$SidecarTimeout,
    [string]$EvidenceDir,
    [string]$BaselineSourceSha,
    [string]$CandidateSourceSha,
    [string]$FixtureIdentity,
    [switch]$ValidateOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-RepositoryRoot {
    return [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
}

function Get-EnvironmentSnapshot {
    $snapshot = [ordered]@{}
    foreach ($item in [Environment]::GetEnvironmentVariables().GetEnumerator() | Sort-Object Key) {
        $snapshot[[string]$item.Key] = Get-Sha256Text ([string]$item.Value)
    }
    return $snapshot
}

function Assert-EnvironmentEqual {
    param([hashtable]$Expected, [hashtable]$Actual)

    if ($Expected.Count -ne $Actual.Count) {
        throw 'environment changed during benchmark setup'
    }
    foreach ($key in $Expected.Keys) {
        if (-not $Actual.Contains($key) -or $Expected[$key] -cne $Actual[$key]) {
            throw "environment changed for $key"
        }
    }
}

function ConvertTo-SidecarTimeout {
    param([string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw 'SidecarTimeout is required'
    }
    if ($Value -match '^(\d+)(ms|s|m|h)$') {
        $amount = [int64]$Matches[1]
        $unit = $Matches[2]
        $timeout = switch ($unit) {
            'ms' { [TimeSpan]::FromMilliseconds($amount) }
            's' { [TimeSpan]::FromSeconds($amount) }
            'm' { [TimeSpan]::FromMinutes($amount) }
            'h' { [TimeSpan]::FromHours($amount) }
        }
    } else {
        try {
            $timeout = [TimeSpan]::Parse($Value, [Globalization.CultureInfo]::InvariantCulture)
        } catch {
            throw "SidecarTimeout $Value must be a positive duration such as 300s"
        }
    }
    if ($timeout -le [TimeSpan]::Zero) {
        throw 'SidecarTimeout must be positive'
    }
    return $timeout
}

function Get-Sha256Text {
    param([string]$Text)

    $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
    return ([Security.Cryptography.SHA256]::HashData($bytes) | ForEach-Object ToString x2) -join ''
}

function Add-HashBytes {
    param(
        [Security.Cryptography.HashAlgorithm]$Hash,
        [byte[]]$Bytes
    )

    if ($Bytes.Length -gt 0) {
        [void]$Hash.TransformBlock($Bytes, 0, $Bytes.Length, $Bytes, 0)
    }
}

function Get-TreeDigest {
    param(
        [string]$Root,
        [switch]$OutputStateOnly
    )

    $rootPath = [IO.Path]::GetFullPath($Root)
    $files = Get-ChildItem -LiteralPath $rootPath -Recurse -Force -File |
        Where-Object {
            $relative = [IO.Path]::GetRelativePath($rootPath, $_.FullName)
            if ($relative -match '(^|[\\/])\.git([\\/]|$)') {
                return $false
            }
            if (-not $OutputStateOnly) {
                return $true
            }
            if ($relative -match '(^|[\\/])(node_modules|\.rotor)([\\/]|$)') {
                return $false
            }
            $first = ($relative -split '[\\/]')[0]
            return $first -in @('out', 'out-test', 'out-tsc') -or $_.Name -like '*.rbxtsc.tsbuildinfo'
        } |
        Sort-Object FullName

    $hash = [Security.Cryptography.SHA256]::Create()
    $separator = [byte[]](0)
    $buffer = New-Object byte[] 65536
    try {
        foreach ($file in $files) {
            $relative = [IO.Path]::GetRelativePath($rootPath, $file.FullName).Replace('\', '/')
            Add-HashBytes $hash ([Text.Encoding]::UTF8.GetBytes($relative))
            Add-HashBytes $hash $separator
            $stream = [IO.File]::OpenRead($file.FullName)
            try {
                while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    if ($read -eq $buffer.Length) {
                        Add-HashBytes $hash $buffer
                    } else {
                        Add-HashBytes $hash $buffer[0..($read - 1)]
                    }
                }
            } finally {
                $stream.Dispose()
            }
            Add-HashBytes $hash $separator
        }
        [void]$hash.TransformFinalBlock([byte[]]::new(0), 0, 0)
        return ($hash.Hash | ForEach-Object ToString x2) -join ''
    } finally {
        $hash.Dispose()
    }
}

function Test-AllowedColdStateTarget {
    param([string]$WorkCopy, [string]$Target)

    $root = [IO.Path]::GetFullPath($WorkCopy).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    $path = [IO.Path]::GetFullPath($Target)
    if (-not $path.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        return $false
    }
    $relative = [IO.Path]::GetRelativePath($root, $path).Replace('\', '/')
    if ($relative -in @('out', 'out-test', 'out-tsc')) {
        return $true
    }
    if ($relative -match '(^|/)(node_modules|\.rotor|\.git)(/|$)') {
        return $false
    }
    return [IO.Path]::GetFileName($path) -like '*.rbxtsc.tsbuildinfo'
}

function Reset-ColdState {
    param([string]$WorkCopy)

    foreach ($name in @('out', 'out-test', 'out-tsc')) {
        $target = Join-Path $WorkCopy $name
        if ((Test-Path -LiteralPath $target) -and -not (Test-AllowedColdStateTarget $WorkCopy $target)) {
            throw "refusing undeclared cold-state cleanup target $target"
        }
        if (Test-Path -LiteralPath $target) {
            Assert-SafeCleanupTarget $target $WorkCopy
            Remove-Item -LiteralPath $target -Recurse -Force
        }
    }
    foreach ($state in Get-ChildItem -LiteralPath $WorkCopy -Recurse -Force -File -Filter '*.rbxtsc.tsbuildinfo') {
        if (Test-AllowedColdStateTarget $WorkCopy $state.FullName) {
            Assert-SafeCleanupTarget $state.FullName $WorkCopy
            Remove-Item -LiteralPath $state.FullName -Force
        }
    }
}

function Get-GitState {
    param([string]$Path)

    $inside = & git -C $Path rev-parse --is-inside-work-tree 2>$null
    if ($LASTEXITCODE -ne 0 -or $inside -ne 'true') {
        return $null
    }
    $status = & git -C $Path status --porcelain=v1 2>$null
    if ($LASTEXITCODE -ne 0) {
        throw "cannot inspect git state at $Path"
    }
    return ($status -join "`n")
}

function Assert-CleanGitWorktree {
    param([string]$Path, [string]$Name)

    $state = Get-GitState $Path
    if ($null -ne $state -and $state -ne '') {
        throw "$Name is a dirty git worktree"
    }
    return $state
}

function Get-GitRoot {
    param([string]$Path)

    $root = & git -C $Path rev-parse --show-toplevel 2>$null
    if ($LASTEXITCODE -ne 0) {
        return $null
    }
    return [IO.Path]::GetFullPath(($root -join "`n"))
}

function Test-PathInside {
    param([string]$Child, [string]$Parent)

    $childPath = [IO.Path]::GetFullPath($Child)
    $parentPath = [IO.Path]::GetFullPath($Parent).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    return $childPath.StartsWith($parentPath + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)
}

function Assert-SafeCleanupTarget {
    param([string]$Target, [string]$Root)

    $path = [IO.Path]::GetFullPath($Target)
    $rootPath = [IO.Path]::GetFullPath($Root)
    if (-not (Test-PathInside $path $rootPath)) {
        throw "cleanup target is outside owned root: $path"
    }
    Assert-NoReparseAncestors $path
    if (Test-Path -LiteralPath $path -PathType Container) {
        foreach ($item in Get-ChildItem -LiteralPath $path -Recurse -Force) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "cleanup tree contains a reparse point: $($item.FullName)"
            }
        }
    }
}

function Assert-NoReparseAncestors {
    param([string]$Path)

    # Lexical absolute paths alone do not contain junctions.
    for ($ancestor = [IO.Path]::GetFullPath($Path); $ancestor; $ancestor = [IO.Path]::GetDirectoryName($ancestor)) {
        if ((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "path traverses a reparse point: $ancestor"
        }
    }
}

function Get-GitIdentity {
    param([string]$Path)

    $root = Get-GitRoot $Path
    if ($null -eq $root) { return $null }
    $sha = & git -C $root rev-parse HEAD 2>$null
    if ($LASTEXITCODE -ne 0) { throw "cannot inspect git HEAD at $root" }
    return [ordered]@{ root = $root; sha = ($sha -join ''); status = Get-GitState $root }
}

function Assert-BenchmarkInputs {
    param([string]$RepositoryRoot)

    foreach ($input in @(
            @{ Name = 'BaselineExe'; Value = $BaselineExe; Kind = 'Leaf' },
            @{ Name = 'CandidateExe'; Value = $CandidateExe; Kind = 'Leaf' },
            @{ Name = 'Fixture'; Value = $Fixture; Kind = 'Container' },
            @{ Name = 'EvidenceDir'; Value = $EvidenceDir; Kind = 'Container' }
        )) {
        if ([string]::IsNullOrWhiteSpace([string]$input.Value)) {
            throw "$($input.Name) is required"
        }
    }
    if (-not (Test-Path -LiteralPath $BaselineExe -PathType Leaf)) {
        throw "BaselineExe does not exist: $BaselineExe"
    }
    if (-not (Test-Path -LiteralPath $CandidateExe -PathType Leaf)) {
        throw "CandidateExe does not exist: $CandidateExe"
    }
    if (-not (Test-Path -LiteralPath $Fixture -PathType Container)) {
        throw "Fixture does not exist: $Fixture"
    }
    foreach ($path in @($RepositoryRoot, $BaselineExe, $CandidateExe, $Fixture, $EvidenceDir)) { Assert-NoReparseAncestors $path }
    foreach ($item in Get-ChildItem -LiteralPath $Fixture -Recurse -Force) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "fixture contains a reparse point: $($item.FullName)" }
    }
    if ($ColdPairs -lt 1 -or $NoChangeRuns -lt 1) {
        throw 'ColdPairs and NoChangeRuns must both be positive'
    }
    [void](ConvertTo-SidecarTimeout $SidecarTimeout)
    $trackedRoots = [Collections.Generic.List[string]]::new()
    foreach ($path in @($RepositoryRoot, $Fixture, (Split-Path -LiteralPath $BaselineExe), (Split-Path -LiteralPath $CandidateExe))) {
        $root = Get-GitRoot $path
        if ($null -eq $root) {
            $root = [IO.Path]::GetFullPath($path)
        }
        if (-not $trackedRoots.Contains($root)) {
            $trackedRoots.Add($root)
        }
    }
    foreach ($root in $trackedRoots) {
        if (([IO.Path]::GetFullPath($EvidenceDir).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar) -eq $root.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)) -or (Test-PathInside $EvidenceDir $root)) {
            throw "EvidenceDir must be outside benchmark input root $root"
        }
    }
    $states = [Collections.Generic.List[object]]::new()
    foreach ($root in $trackedRoots) {
        [void](Assert-CleanGitWorktree $root 'Benchmark input worktree')
        $states.Add([ordered]@{ Root = $root; Identity = Get-GitIdentity $root })
    }
    return @($states)
}

function Initialize-ProcessOwnership {
    if ('RotorBench.OwnedProcess' -as [type]) { return }
    # A job retains ownership of descendants even if the parent exits while a
    # descendant still holds a redirected stream. No name/PID-wide taskkill.
    Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
namespace RotorBench {
    public sealed class Capture {
        readonly StringBuilder text = new StringBuilder();
        public Task Completion { get; }
        public string Text { get { lock (text) return text.ToString(); } }
        public Capture(StreamReader reader) { Completion = Read(reader); }
        async Task Read(StreamReader reader) {
            var buffer = new char[4096];
            int count;
            while ((count = await reader.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false)) != 0) {
                lock (text) text.Append(buffer, 0, count);
            }
        }
    }
    public sealed class OwnedProcess : IDisposable {
        [StructLayout(LayoutKind.Sequential)] struct BasicLimits {
            public long PerProcessUserTimeLimit, PerJobUserTimeLimit;
            public uint LimitFlags;
            public UIntPtr MinimumWorkingSetSize, MaximumWorkingSetSize;
            public uint ActiveProcessLimit;
            public UIntPtr Affinity;
            public uint PriorityClass, SchedulingClass;
        }
        [StructLayout(LayoutKind.Sequential)] struct IoCounters {
            public ulong ReadOperationCount, WriteOperationCount, OtherOperationCount;
            public ulong ReadTransferCount, WriteTransferCount, OtherTransferCount;
        }
        [StructLayout(LayoutKind.Sequential)] struct ExtendedLimits {
            public BasicLimits BasicLimitInformation;
            public IoCounters IoInfo;
            public UIntPtr ProcessMemoryLimit, JobMemoryLimit, PeakProcessMemoryUsed, PeakJobMemoryUsed;
        }
        [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateJobObject(IntPtr attributes, string name);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr job, int infoClass, ref ExtendedLimits info, uint length);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool TerminateJobObject(IntPtr job, uint exitCode);
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
        IntPtr job;
        bool attached, complete;
        public Process Process { get; } = new Process();
        void SetKillOnClose(bool enabled) {
            var limits = new ExtendedLimits();
            limits.BasicLimitInformation.LimitFlags = enabled ? 0x2000u : 0u;
            if (!SetInformationJobObject(job, 9, ref limits, (uint)Marshal.SizeOf<ExtendedLimits>()))
                throw new System.ComponentModel.Win32Exception();
        }
        public OwnedProcess() {
            if (OperatingSystem.IsWindows()) {
                job = CreateJobObject(IntPtr.Zero, null);
                if (job == IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
                try { SetKillOnClose(true); } catch { CloseHandle(job); job = IntPtr.Zero; throw; }
            }
        }
        public void Attach() {
            if (job != IntPtr.Zero) {
                if (!AssignProcessToJobObject(job, Process.Handle)) throw new System.ComponentModel.Win32Exception();
                attached = true;
            }
        }
        public void Kill() {
            if (attached) {
                if (!TerminateJobObject(job, 1)) throw new System.ComponentModel.Win32Exception();
            } else if (!Process.HasExited) { Process.Kill(true); }
        }
        public void Complete() {
            if (job != IntPtr.Zero) SetKillOnClose(false);
            complete = true;
        }
        public void Dispose() {
            try { if (!complete) { try { Kill(); } catch (InvalidOperationException) {} } }
            finally { if (job != IntPtr.Zero) CloseHandle(job); Process.Dispose(); }
        }
    }
}
'@
}

function Write-ProcessEvidence {
    param([string]$StdoutPath, [string]$StderrPath, [string]$Stdout, [string]$Stderr)

    [IO.File]::WriteAllText($StdoutPath, $Stdout, [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($StderrPath, $Stderr, [Text.UTF8Encoding]::new($false))
}

function Invoke-ProcessWithTimeout {
    param(
        [string]$Executable,
        [string[]]$Arguments,
        [string]$WorkingDirectory,
        [TimeSpan]$Timeout,
        [string]$StdoutPath,
        [string]$StderrPath
    )

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Executable
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $Arguments) {
        [void]$startInfo.ArgumentList.Add($argument)
    }
    Initialize-ProcessOwnership
    $owned = [RotorBench.OwnedProcess]::new()
    $process = $owned.Process
    $process.StartInfo = $startInfo
    $stdoutCapture = $stderrCapture = $null
    $result = [ordered]@{ DurationMS = $null; ExitCode = $null; Status = 'incomplete'; Error = $null; EvidencePersistenceMS = $null }
    $stopwatch = [Diagnostics.Stopwatch]::new()
    try {
        $stopwatch.Start()
        if (-not $process.Start()) {
            throw "could not start $Executable"
        }
        try { $owned.Attach() }
        finally {
            $stdoutCapture = [RotorBench.Capture]::new($process.StandardOutput)
            $stderrCapture = [RotorBench.Capture]::new($process.StandardError)
        }
        $completion = [Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($process.WaitForExitAsync(), $stdoutCapture.Completion, $stderrCapture.Completion))
        # Short waits let PowerShell deliver cancellation and execute finally.
        # A completed task wakes immediately, without a polling-sized timing tail.
        while ($true) {
            $milliseconds = [Math]::Min(100, [Math]::Max(0, [Math]::Ceiling($Timeout.TotalMilliseconds - $stopwatch.Elapsed.TotalMilliseconds)))
            if ($completion.Wait([int]$milliseconds)) { break }
            if ($stopwatch.Elapsed -ge $Timeout) {
                $result.Status = 'timeout'
                throw "command or redirected streams timed out after ${Timeout}"
            }
        }
        $stopwatch.Stop()
        $owned.Complete()
        $result.Status = 'completed'
    } catch {
        $result.Error = $_.Exception.Message
    } finally {
        $stopwatch.Stop()
        $result.DurationMS = [Math]::Max(1, [int64][Math]::Round($stopwatch.Elapsed.TotalMilliseconds))
        try {
            if ($result.Status -ne 'completed') {
                try { $owned.Kill() } catch { $result.Error += "; cleanup: $($_.Exception.Message)" }
                if ($null -ne $stdoutCapture) {
                    try {
                        $drain = [Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($stdoutCapture.Completion, $stderrCapture.Completion))
                        if (-not $drain.Wait(5000)) { $result.Error += '; redirected streams did not close after termination' }
                    } catch { $result.Error += "; stream capture: $($_.Exception.Message)" }
                }
            }
            # Killing this invocation's job closes inherited pipes, preserving
            # output already produced on timeout/cancellation as well as success.
            $stdout = if ($null -ne $stdoutCapture) { $stdoutCapture.Text } else { '' }
            $stderr = if ($null -ne $stderrCapture) { $stderrCapture.Text } else { '' }
            if ($null -ne $stdoutCapture -and $process.HasExited) { $result.ExitCode = $process.ExitCode }
            $persistence = [Diagnostics.Stopwatch]::StartNew()
            try { Write-ProcessEvidence $StdoutPath $StderrPath $stdout $stderr }
            catch { $result.Status = 'incomplete'; $result.Error += "; evidence persistence: $($_.Exception.Message)" }
            finally { $persistence.Stop(); $result.EvidencePersistenceMS = $persistence.Elapsed.TotalMilliseconds }
        } finally { $owned.Dispose() }
    }
    return $result
}

function Invoke-ScoredBuild {
    param(
        [string]$Executable,
        [string]$WorkCopy,
        [TimeSpan]$Timeout,
        [string]$EvidencePath,
        [string]$JournalPath,
        [System.Collections.IDictionary]$Invocation,
        [string[]]$ExtraArguments = @()
    )

    $entry = [ordered]@{
        event = 'started'; invocation = $Invocation; at_utc = [DateTime]::UtcNow.ToString('o')
        status = 'incomplete'; executable = $Executable; working_directory = $WorkCopy
        stdout = "$EvidencePath.stdout.json"; stderr = "$EvidencePath.stderr.txt"
    }
    Add-InvocationRecord $JournalPath $entry
    $result = $null
    $entry.error = $null
    try {
        $result = Invoke-ProcessWithTimeout $Executable (@('build', '--project', $WorkCopy, '--json') + $ExtraArguments) $WorkCopy $Timeout $entry.stdout $entry.stderr
        if ($result.Status -ne 'completed') { throw $result.Error }
        $publicResult = [IO.File]::ReadAllText($entry.stdout) | ConvertFrom-Json -AsHashtable
        if ($publicResult -isnot [System.Collections.IDictionary] -or $publicResult.ok -isnot [bool] -or $publicResult.diagnostics -isnot [array]) {
            throw 'prebuilt binary did not emit a public --json result object with ok and diagnostics'
        }
        if ($result.ExitCode -ne 0 -or -not $publicResult.ok) { throw "build failed (exit_code=$($result.ExitCode), ok=$($publicResult.ok))" }
        $result.DiagnosticsDigest = Get-Sha256Text (ConvertTo-Json -InputObject $publicResult.diagnostics -Compress -Depth 32)
        $result.OutputTreeDigest = Get-TreeDigest $WorkCopy -OutputStateOnly
    } catch {
        $entry.error = $_.Exception.Message
        if ($null -ne $result) {
            if ($result.Status -eq 'completed') { $result.Status = 'incomplete' }
            $result.Error = $_.Exception.Message
        }
        throw
    } finally {
        $entry.event = 'finished'
        $entry.at_utc = [DateTime]::UtcNow.ToString('o')
        $entry.status = if ($null -ne $result -and $result.Status -eq 'completed') { 'completed' } else { 'incomplete' }
        $entry.result = $result
        Add-InvocationRecord $JournalPath $entry
    }
    return $result
}

function Add-InvocationRecord {
    param([string]$Path, [System.Collections.IDictionary]$Record)

    [IO.File]::AppendAllText($Path, (ConvertTo-Json -InputObject $Record -Compress -Depth 32) + "`n", [Text.UTF8Encoding]::new($false))
}

function New-ManifestRecord {
    param(
        [int]$Pair,
        [string]$Phase,
        [string]$Order,
        [string]$Binary,
        [hashtable]$Result
    )

    return [ordered]@{
        pair = $Pair
        phase = $Phase
        order = $Order
        binary = $Binary
        duration_ms = $Result.DurationMS
        exit_code = $Result.ExitCode
        diagnostics_digest = $Result.DiagnosticsDigest
        output_tree_digest = $Result.OutputTreeDigest
    }
}

function Get-OrderForPair {
    param([int]$Pair)

    if ($Pair % 2 -eq 1) {
        return 'AB'
    }
    return 'BA'
}

function Get-WindowsMachineMetadata {
    param([string]$TimeoutValue, [hashtable]$Environment)

    $os = Get-CimInstance Win32_OperatingSystem
    $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
    $computer = Get-CimInstance Win32_ComputerSystem
    $storage = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='$($env:SystemDrive)'" | Select-Object -First 1
    $power = (& powercfg /getactivescheme 2>$null) -join ' '
    return [ordered]@{
        os = $os.Caption
        version = $os.Version
        cpu = $cpu.Name
        ram_bytes = [int64]$computer.TotalPhysicalMemory
        storage = "$($storage.DeviceID) size=$($storage.Size) free=$($storage.FreeSpace)"
        power = $power
        run_order = @('AB', 'BA')
        sidecar_timeout = $TimeoutValue
        environment = $Environment
    }
}

function Assert-EvaluatorPrerequisites {
    param([string]$RepositoryRoot)

    $go = Get-Command go -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $go) { throw 'perfcompare requires Go on PATH before any benchmark runs' }
    foreach ($relative in @('go.mod', 'tools/perfcompare/main.go', 'tools/perfcompare/schema.go', 'tools/perfcompare/verdict.go')) {
        if (-not (Test-Path -LiteralPath (Join-Path $RepositoryRoot $relative) -PathType Leaf)) {
            throw "perfcompare prerequisite missing in RepositoryRoot: $relative"
        }
    }
    return $go.Source
}

function Invoke-PerfCompare {
    param([string]$RepositoryRoot, [string]$ManifestPath, [string]$VerdictPath)

    $go = Assert-EvaluatorPrerequisites $RepositoryRoot
    $result = Invoke-ProcessWithTimeout $go @('run', './tools/perfcompare', '--input', $ManifestPath, '--output', $VerdictPath) $RepositoryRoot ([TimeSpan]::FromMinutes(5)) "$VerdictPath.stdout.txt" "$VerdictPath.stderr.txt"
    if ($result.Status -ne 'completed') { throw "perfcompare did not complete: $($result.Error)" }
    if (-not (Test-Path -LiteralPath $VerdictPath -PathType Leaf)) { throw 'perfcompare did not produce a verdict' }
    $verdict = Get-Content -LiteralPath $VerdictPath -Raw | ConvertFrom-Json
    if ($verdict.schema -ne 1 -or $verdict.status -notin @('PASS', 'FAIL') -or (($result.ExitCode -eq 0) -ne ($verdict.status -eq 'PASS'))) {
        throw 'perfcompare exit code and verdict are inconsistent'
    }
    return [ordered]@{ ExitCode = $result.ExitCode; Status = $verdict.status; Output = [IO.File]::ReadAllText("$VerdictPath.stderr.txt") }
}

function Remove-TestOwnedWorkCopies {
    param([string]$EvidencePath, [string]$WorkRoot)

    if ((Test-Path -LiteralPath $WorkRoot) -and (Test-PathInside $WorkRoot $EvidencePath) -and ([IO.Path]::GetFileName($WorkRoot) -match '^work-[0-9a-f]{32}$|^work-validation$')) {
        Assert-SafeCleanupTarget $WorkRoot $EvidencePath
        Remove-Item -LiteralPath $WorkRoot -Recurse -Force
    }
}

function Get-BinaryIdentity {
    param([string]$Executable, [string]$SourceSha, [string]$GoExecutable)

    # Build flags can contain credentials. Keep a digest of the full Go build
    # identity, and expose only fixed, public toolchain/VCS/target fields.
    $buildInfo = (& $GoExecutable version -m $Executable 2>$null) -join "`n"
    $fields = [ordered]@{}
    $settings = [ordered]@{}
    foreach ($line in ($buildInfo -split "`n")) {
        if ($line -match ': (go[0-9][^\s]*)$') { $fields.go_toolchain = $Matches[1] }
        if ($line -match '^\s*build\s+(vcs\.revision|vcs\.time|vcs\.modified|GOOS|GOARCH|CGO_ENABLED|-compiler|-buildmode|-trimpath)=(\S+)$') {
            $fields[$Matches[1]] = $Matches[2]
        }
        if ($line -match '^\s*build\s+([-A-Za-z0-9_.]+)=(.*)$') { $settings[$Matches[1]] = Get-Sha256Text $Matches[2] }
    }
    if ($SourceSha -and $fields.Contains('vcs.revision') -and $fields['vcs.revision'] -ne $SourceSha) {
        throw "provided source SHA disagrees with embedded VCS revision: $Executable"
    }
    return [ordered]@{
        path = $Executable; sha256 = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash
        provided_source_sha = $SourceSha; observed_build = $fields; build_setting_hashes = $settings; build_info_sha256 = Get-Sha256Text $buildInfo
    }
}

function Get-FixtureProvenance {
    param([string]$Root)

    $files = [ordered]@{}
    foreach ($relative in @('package.json', 'package-lock.json', 'bun.lock', 'bun.lockb', 'pnpm-lock.yaml', 'yarn.lock', 'tsconfig.json', 'rotor.toml')) {
        $path = Join-Path $Root $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) { $files[$relative] = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash }
    }
    $packages = [ordered]@{}
    foreach ($name in @('typescript', 'roblox-ts', '@rbxts/types', '@rbxts/compiler-types', 'rbxts-transformer-flamework')) {
        $path = Join-Path $Root "node_modules/$name/package.json"
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $package = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable
            $packages[$name] = [ordered]@{ version = $package.version; manifest_sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash }
        }
    }
    return [ordered]@{ input_file_hashes = $files; observed_package_versions = $packages }
}

function Get-InputIdentity {
    param([object[]]$TrackedWorktrees, [string]$GoExecutable)

    $repositories = @(foreach ($worktree in $TrackedWorktrees) { [ordered]@{ root = $worktree.Root; identity = Get-GitIdentity $worktree.Root } })
    return [ordered]@{
        baseline = Get-BinaryIdentity $BaselineExe $BaselineSourceSha $GoExecutable
        candidate = Get-BinaryIdentity $CandidateExe $CandidateSourceSha $GoExecutable
        fixture = [ordered]@{ path = $Fixture; sha256 = Get-TreeDigest $Fixture; provided_identity = $FixtureIdentity; provenance = Get-FixtureProvenance $Fixture }
        repositories = $repositories
    }
}

function Assert-InputIdentityEqual {
    param([System.Collections.IDictionary]$Before, [System.Collections.IDictionary]$After)

    if ((ConvertTo-Json -InputObject $Before -Depth 32 -Compress) -cne (ConvertTo-Json -InputObject $After -Depth 32 -Compress)) {
        throw 'benchmark input identity changed (binary, canonical fixture, repository HEAD/status, or observed build identity)'
    }
}

function Write-RunStatus {
    param([string]$Path, [System.Collections.IDictionary]$Run)

    # Replace only the status snapshot; invocations remain an append-only log.
    $temporary = "$Path.tmp"
    [IO.File]::WriteAllText($temporary, (ConvertTo-Json -InputObject $Run -Depth 32), [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporary, $Path, $true)
}

function Invoke-RealBenchmark {
    param([string]$RepositoryRoot)

    if (-not $IsWindows) {
        throw 'Windows benchmark runs require Windows. Use -ValidateOnly on other platforms.'
    }
    $go = Assert-EvaluatorPrerequisites $RepositoryRoot
    foreach ($variable in @('BaselineExe', 'CandidateExe', 'Fixture', 'EvidenceDir')) {
        $value = Get-Variable -Name $variable -ValueOnly
        if ([string]::IsNullOrWhiteSpace($value)) { throw "$variable is required" }
        Set-Variable -Name $variable -Value ([IO.Path]::GetFullPath($value)) -Scope Script
    }
    foreach ($sha in @($BaselineSourceSha, $CandidateSourceSha)) {
        if ($sha -and $sha -notmatch '^[0-9a-fA-F]{40}$') { throw 'provided source SHA must be a full 40-character Git SHA' }
    }
    $trackedWorktrees = Assert-BenchmarkInputs $RepositoryRoot
    $timeout = ConvertTo-SidecarTimeout $SidecarTimeout
    $fixturePath = [IO.Path]::GetFullPath($Fixture)
    $evidencePath = [IO.Path]::GetFullPath($EvidenceDir)
    if ((Test-Path -LiteralPath $evidencePath) -and @(Get-ChildItem -LiteralPath $evidencePath -Force).Count -gt 0) {
        throw 'EvidenceDir must be empty; use a fresh directory to preserve previous evidence'
    }
    [IO.Directory]::CreateDirectory($evidencePath) | Out-Null
    $workRoot = Join-Path $evidencePath ("work-" + [Guid]::NewGuid().ToString('N'))
    $baselineWork = Join-Path $workRoot 'baseline'
    $candidateWork = Join-Path $workRoot 'candidate'
    $manifestPath = Join-Path $evidencePath 'windows-build-performance-manifest.json'
    $verdictPath = Join-Path $evidencePath 'windows-build-performance-verdict.json'
    $journalPath = Join-Path $evidencePath 'invocations.ndjson'
    $runPath = Join-Path $evidencePath 'run.json'
    $pendingManifest = "$manifestPath.pending"
    $environment = Get-EnvironmentSnapshot
    $oldSidecarTimeout = [Environment]::GetEnvironmentVariable('ROTOR_SIDECAR_TIMEOUT', 'Process')
    $records = [Collections.Generic.List[object]]::new()
    $run = [ordered]@{
        schema = 1; status = 'incomplete'; started_utc = [DateTime]::UtcNow.ToString('o'); finished_utc = $null
        error = $null; before = $null; after = $null; evaluation = $null
        invocation_journal = $journalPath
        scope = 'single-project fresh-process; output/state reset; warm OS cache; output digest excludes include, custom outputs and mtimes; canonical digest covers all files except .git'
        tools = [ordered]@{ powershell = $PSVersionTable.PSVersion.ToString(); go = (& $go version) -join ' '; git = (& git --version) -join ' ' }
        machine = $null
    }
    $node = Get-Command node -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    $run.tools.node = if ($null -ne $node) { (& $node.Source --version) -join ' ' } else { 'unavailable' }
    Write-RunStatus $runPath $run
    try {
        $env:ROTOR_SIDECAR_TIMEOUT = $SidecarTimeout
        $environment = Get-EnvironmentSnapshot
        $run.before = Get-InputIdentity $trackedWorktrees $go
        $run.machine = Get-WindowsMachineMetadata $SidecarTimeout $environment
        Write-RunStatus $runPath $run
        [IO.Directory]::CreateDirectory($workRoot) | Out-Null
        Copy-Item -LiteralPath $fixturePath -Destination $baselineWork -Recurse -Force
        Copy-Item -LiteralPath $fixturePath -Destination $candidateWork -Recurse -Force
        if ((Get-TreeDigest $baselineWork) -cne $run.before.fixture.sha256 -or (Get-TreeDigest $candidateWork) -cne $run.before.fixture.sha256) {
            throw 'work copy does not match the canonical fixture'
        }
        foreach ($phaseInfo in @(@{ Name = 'cold'; Count = $ColdPairs }, @{ Name = 'no_change'; Count = $NoChangeRuns })) {
            for ($pair = 1; $pair -le $phaseInfo.Count; $pair++) {
                $order = Get-OrderForPair $pair
                if ($phaseInfo.Name -eq 'cold') {
                    Reset-ColdState $baselineWork
                    Reset-ColdState $candidateWork
                }
                $sequence = if ($order -eq 'AB') { @('baseline', 'candidate') } else { @('candidate', 'baseline') }
                foreach ($binary in $sequence) {
                    $workCopy = if ($binary -eq 'baseline') { $baselineWork } else { $candidateWork }
                    $executable = if ($binary -eq 'baseline') { $BaselineExe } else { $CandidateExe }
                    $prefix = Join-Path $evidencePath ("$($phaseInfo.Name)-$pair-$binary")
                    $invocation = [ordered]@{ pair = $pair; phase = $phaseInfo.Name; order = $order; binary = $binary; scored = $true }
                    $result = Invoke-ScoredBuild $executable $workCopy $timeout $prefix $journalPath $invocation
                    $records.Add((New-ManifestRecord $pair $phaseInfo.Name $order $binary $result))
                }
            }
        }
        $timingPath = Join-Path $evidencePath 'candidate-timings.json'
        [void](Invoke-ScoredBuild $CandidateExe $candidateWork $timeout (Join-Path $evidencePath 'candidate-timings') $journalPath ([ordered]@{ phase = 'diagnostic'; binary = 'candidate'; scored = $false }) @('--timings', $timingPath))
        if (-not (Test-Path -LiteralPath $timingPath -PathType Leaf)) { throw 'candidate diagnostic did not write the requested timing file' }
        $run.after = Get-InputIdentity $trackedWorktrees $go
        Assert-InputIdentityEqual $run.before $run.after
        Assert-EnvironmentEqual $environment (Get-EnvironmentSnapshot)
        $manifest = [ordered]@{
            schema = 1
            machine = $run.machine
            baseline = [ordered]@{ revision = "sha256:$($run.before.baseline.sha256)"; command = "$BaselineExe build --project <work-copy> --json" }
            candidate = [ordered]@{ revision = "sha256:$($run.before.candidate.sha256)"; command = "$CandidateExe build --project <work-copy> --json" }
            records = @($records)
        }
        $manifest | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $pendingManifest -Encoding utf8NoBOM
        $evaluation = Invoke-PerfCompare $RepositoryRoot $pendingManifest $verdictPath
        $run.evaluation = $evaluation
        $run.after = Get-InputIdentity $trackedWorktrees $go
        Assert-InputIdentityEqual $run.before $run.after
        Assert-EnvironmentEqual $environment (Get-EnvironmentSnapshot)
        if (Test-Path -LiteralPath $verdictPath) {
            $verdict = Get-Content -LiteralPath $verdictPath -Raw | ConvertFrom-Json
            if (@($verdict.reasons | Where-Object code -eq 'invalid_manifest').Count -gt 0) {
                throw "perfcompare rejected invalid evidence: $($evaluation.Output)"
            }
        }
        [IO.File]::Move($pendingManifest, $manifestPath)
        $run.status = 'evaluated'
        if ($evaluation.ExitCode -ne 0) {
            throw "perfcompare rejected benchmark evidence: $($evaluation.Output)"
        }
    } catch {
        $run.error = $_.Exception.Message
        throw
    } finally {
        try {
            if ($run.status -eq 'incomplete' -and $null -ne $run.before) {
                try { $run.after = Get-InputIdentity $trackedWorktrees $go }
                catch { $run.error += "; final identity unavailable: $($_.Exception.Message)"; $run.status = 'incomplete' }
            }
            $run.finished_utc = [DateTime]::UtcNow.ToString('o')
            Write-RunStatus $runPath $run
        } finally {
            if ($null -eq $oldSidecarTimeout) {
                Remove-Item Env:ROTOR_SIDECAR_TIMEOUT -ErrorAction SilentlyContinue
            } else {
                $env:ROTOR_SIDECAR_TIMEOUT = $oldSidecarTimeout
            }
            try { Remove-TestOwnedWorkCopies $evidencePath $workRoot }
            catch {
                $run.cleanup_error = $_.Exception.Message
                Write-RunStatus $runPath $run
                throw
            }
        }
    }
}

function Invoke-HarnessValidation {
    param([string]$RepositoryRoot)

    [void](Assert-EvaluatorPrerequisites $RepositoryRoot)
    $validationRoot = if ($EvidenceDir) { [IO.Path]::GetFullPath($EvidenceDir) } else { Join-Path ([IO.Path]::GetTempPath()) ("rotor-bench-validation-" + [Guid]::NewGuid().ToString('N')) }
    [IO.Directory]::CreateDirectory($validationRoot) | Out-Null
    $fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ("rotor-fixture-validation-" + [Guid]::NewGuid().ToString('N'))
    $workCopy = Join-Path $validationRoot 'work-validation'
    try {
        [IO.Directory]::CreateDirectory((Join-Path $fixtureRoot 'src')) | Out-Null
        [IO.Directory]::CreateDirectory((Join-Path $fixtureRoot 'node_modules')) | Out-Null
        [IO.Directory]::CreateDirectory((Join-Path $fixtureRoot '.rotor')) | Out-Null
        Set-Content -LiteralPath (Join-Path $fixtureRoot 'src/input.ts') -Value 'export const value = 1' -NoNewline
        Set-Content -LiteralPath (Join-Path $fixtureRoot 'node_modules/dependency.rbxtsc.tsbuildinfo') -Value 'preserve' -NoNewline
        Set-Content -LiteralPath (Join-Path $fixtureRoot '.rotor/cache.rbxtsc.tsbuildinfo') -Value 'preserve' -NoNewline
        $fixtureDigest = Get-TreeDigest $fixtureRoot
        if ((Get-OrderForPair 1) -ne 'AB' -or (Get-OrderForPair 2) -ne 'BA') {
            throw 'AB/BA ordering validation failed'
        }
        Copy-Item -LiteralPath $fixtureRoot -Destination $workCopy -Recurse -Force
        foreach ($name in @('out', 'out-test', 'out-tsc')) {
            [IO.Directory]::CreateDirectory((Join-Path $workCopy $name)) | Out-Null
        }
        Set-Content -LiteralPath (Join-Path $workCopy 'src/state.rbxtsc.tsbuildinfo') -Value 'delete' -NoNewline
        Reset-ColdState $workCopy
        if ((Test-Path -LiteralPath (Join-Path $workCopy 'out')) -or (Test-Path -LiteralPath (Join-Path $workCopy 'src/state.rbxtsc.tsbuildinfo')) -or -not (Test-Path -LiteralPath (Join-Path $workCopy 'node_modules/dependency.rbxtsc.tsbuildinfo')) -or -not (Test-Path -LiteralPath (Join-Path $workCopy '.rotor/cache.rbxtsc.tsbuildinfo'))) {
            throw 'cold cleanup allowlist validation failed'
        }
        if ((Get-TreeDigest $fixtureRoot) -ne $fixtureDigest) {
            throw 'canonical fixture immutability validation failed'
        }
        $environment = Get-EnvironmentSnapshot
        Assert-EnvironmentEqual $environment (Get-EnvironmentSnapshot)
        $records = [Collections.Generic.List[object]]::new()
        foreach ($pair in 1..2) {
            $order = Get-OrderForPair $pair
            $first = if ($order -eq 'AB') { 'baseline' } else { 'candidate' }
            $second = if ($first -eq 'baseline') { 'candidate' } else { 'baseline' }
            $result = @{ DurationMS = 1; ExitCode = 0; DiagnosticsDigest = 'diagnostics'; OutputTreeDigest = 'output' }
            $records.Add((New-ManifestRecord $pair 'cold' $order $first $result))
            $records.Add((New-ManifestRecord $pair 'cold' $order $second $result))
        }
        $manifestPath = Join-Path $validationRoot 'windows-build-performance-validation-manifest.json'
        $verdictPath = Join-Path $validationRoot 'windows-build-performance-validation-verdict.json'
        $manifest = [ordered]@{
            schema = 1
            machine = [ordered]@{ os = 'validation'; version = '1'; cpu = 'validation'; ram_bytes = 1; storage = 'validation'; power = 'validation'; run_order = @('AB', 'BA'); sidecar_timeout = '300s'; environment = $environment }
            baseline = [ordered]@{ revision = 'validation'; command = 'validation baseline' }
            candidate = [ordered]@{ revision = 'validation'; command = 'validation candidate' }
            records = @($records)
        }
        $manifest | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
        $evaluation = Invoke-PerfCompare $RepositoryRoot $manifestPath $verdictPath
        if ($null -eq $evaluation -or $evaluation.ExitCode -eq 0 -or $evaluation.Output -notmatch 'cold pairs = 2, want at least 10') {
            throw 'perfcompare schema compatibility validation failed'
        }
        Write-Output 'ValidateOnly: AB/BA ordering, cleanup allowlist, fixture immutability, environment equality, manifest schema, and evaluator rejection passed.'
        Write-Output "ValidateOnly: manifest=$manifestPath"
        Write-Output "ValidateOnly: verdict=$verdictPath"
    } finally {
        if (Test-Path -LiteralPath $fixtureRoot) {
            Assert-SafeCleanupTarget $fixtureRoot ([IO.Path]::GetTempPath())
            Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
        }
        if (-not $EvidenceDir -and (Test-Path -LiteralPath $validationRoot)) {
            Assert-SafeCleanupTarget $validationRoot ([IO.Path]::GetTempPath())
            Remove-Item -LiteralPath $validationRoot -Recurse -Force
        }
    }
}

# Dot-sourcing exposes only harness functions for the targeted stub tests.
if ($MyInvocation.InvocationName -eq '.') { return }

$repositoryRoot = Get-RepositoryRoot
if ($ValidateOnly) {
    Invoke-HarnessValidation $repositoryRoot
    exit 0
}

Invoke-RealBenchmark $repositoryRoot
