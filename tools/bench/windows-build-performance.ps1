#Requires -Version 7.5
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
    [ValidateRange(1, 1073741824)]
    [long]$MaxCaptureBytes = 16777216,
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

    $paths = [string[]]@($files | ForEach-Object { [IO.Path]::GetRelativePath($rootPath, $_.FullName).Replace('\', '/') })
    [Array]::Sort($paths, [StringComparer]::Ordinal)
    $hash = [Security.Cryptography.SHA256]::Create()
    try {
        Add-HashBytes $hash ([Text.Encoding]::UTF8.GetBytes("rotor-tree-digest-v2`0"))
        Add-HashLength $hash $paths.Length
        foreach ($relative in $paths) {
            $pathBytes = [Text.Encoding]::UTF8.GetBytes($relative)
            Add-HashLength $hash $pathBytes.Length
            Add-HashBytes $hash $pathBytes
            $stream = [IO.File]::OpenRead((Join-Path $rootPath $relative))
            try {
                $length = $stream.Length
                Add-HashLength $hash $length
                Add-HashBytes $hash ([Security.Cryptography.SHA256]::HashData($stream))
                if ($stream.Length -ne $length) { throw "file length changed while hashing: $relative" }
            } finally {
                $stream.Dispose()
            }
        }
        [void]$hash.TransformFinalBlock([byte[]]::new(0), 0, 0)
        return 'sha256-tree-v2:' + (($hash.Hash | ForEach-Object ToString x2) -join '')
    } finally {
        $hash.Dispose()
    }
}

function Add-HashLength {
    param([Security.Cryptography.HashAlgorithm]$Hash, [uint64]$Length)

    $bytes = [BitConverter]::GetBytes($Length)
    if (-not [BitConverter]::IsLittleEndian) { [Array]::Reverse($bytes) }
    Add-HashBytes $Hash $bytes
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
    if ('RotorBench.Invocation' -as [type]) { return }
    # JOB_LIST assigns membership inside CreateProcess, before child code runs.
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;
namespace RotorBench {
    public sealed class PublicIdentity {
        public bool Ok { get; }
        public string DiagnosticsDigest { get; }
        public PublicIdentity(bool ok, string digest) { Ok = ok; DiagnosticsDigest = digest; }
    }
    public static class StrictJson {
        static JsonDocument Parse(string json) => JsonDocument.Parse(json, new JsonDocumentOptions {
                AllowTrailingCommas = false, CommentHandling = JsonCommentHandling.Disallow, MaxDepth = 64
            });
        public static void Validate(string json, string kind) {
            using var doc = Parse(json);
            ValidateRoot(doc.RootElement, kind);
        }
        public static PublicIdentity ReadPublicIdentity(string json) {
            using var doc = Parse(json);
            ValidateRoot(doc.RootElement, "public_result");
            using var bytes = new MemoryStream();
            using (var writer = new BinaryWriter(bytes, new UTF8Encoding(false, true), true)) {
                writer.Write(Encoding.ASCII.GetBytes("rotor-diagnostics-v2\0"));
                WriteValue(writer, doc.RootElement.GetProperty("diagnostics"));
            }
            return new PublicIdentity(doc.RootElement.GetProperty("ok").GetBoolean(),
                "sha256-diagnostics-v2:" + Convert.ToHexString(SHA256.HashData(bytes.ToArray())).ToLowerInvariant());
        }
        static void WriteText(BinaryWriter writer, string text) {
            byte[] bytes = new UTF8Encoding(false, true).GetBytes(text);
            writer.Write((ulong)bytes.Length); writer.Write(bytes);
        }
        // Type tags and length/count framing avoid scalar coercion, depth
        // truncation and serialization warnings. Numbers retain exact tokens.
        static void WriteValue(BinaryWriter writer, JsonElement value) {
            switch (value.ValueKind) {
                case JsonValueKind.Object:
                    writer.Write((byte)'o');
                    var properties = new List<JsonProperty>(value.EnumerateObject());
                    properties.Sort((a, b) => StringComparer.Ordinal.Compare(a.Name, b.Name));
                    writer.Write((ulong)properties.Count);
                    foreach (var property in properties) { WriteText(writer, property.Name); WriteValue(writer, property.Value); }
                    break;
                case JsonValueKind.Array:
                    writer.Write((byte)'a'); writer.Write((ulong)value.GetArrayLength());
                    foreach (var element in value.EnumerateArray()) WriteValue(writer, element);
                    break;
                case JsonValueKind.String: writer.Write((byte)'s'); WriteText(writer, value.GetString()); break;
                case JsonValueKind.Number: writer.Write((byte)'n'); WriteText(writer, value.GetRawText()); break;
                case JsonValueKind.True: writer.Write((byte)'t'); break;
                case JsonValueKind.False: writer.Write((byte)'f'); break;
                case JsonValueKind.Null: writer.Write((byte)'z'); break;
                default: throw new FormatException("unsupported JSON value");
            }
        }
        static void ValidateRoot(JsonElement root, string kind) {
            Check(root);
            if (root.ValueKind != JsonValueKind.Object) throw new FormatException("JSON result must be one object");
            if (kind == "public_result") {
                if (!root.TryGetProperty("ok", out var ok) || (ok.ValueKind != JsonValueKind.True && ok.ValueKind != JsonValueKind.False) ||
                    !root.TryGetProperty("diagnostics", out var diagnostics) || diagnostics.ValueKind != JsonValueKind.Array)
                    throw new FormatException("public JSON requires Boolean ok and diagnostics array");
                foreach (var diagnostic in diagnostics.EnumerateArray()) {
                    if (diagnostic.ValueKind != JsonValueKind.Object || !StringField(diagnostic, "message") || !StringField(diagnostic, "severity"))
                        throw new FormatException("JSON diagnostics require object entries with message and severity strings");
                }
                if ((root.TryGetProperty("version", out var release) && release.ValueKind != JsonValueKind.String) ||
                    (root.TryGetProperty("files", out var files) && (!files.TryGetInt64(out long count) || count < 0)) ||
                    (root.TryGetProperty("durationMs", out var elapsed) && (!elapsed.TryGetInt64(out long duration) || duration < 0)))
                    throw new FormatException("invalid public JSON version/files/durationMs fields");
            } else if (kind == "timings") {
                if (!root.TryGetProperty("schemaVersion", out var schema) || !schema.TryGetInt32(out int version) || version != 2 ||
                    !root.TryGetProperty("ok", out var ok) || ok.ValueKind != JsonValueKind.True ||
                    !root.TryGetProperty("totalMs", out var total) || !total.TryGetInt64(out long time) || time < 0 ||
                    !root.TryGetProperty("stageSemantics", out var semantics) || semantics.ValueKind != JsonValueKind.String || semantics.GetString() != "workMs" ||
                    !ObjectField(root, "stages") || !ObjectField(root, "counts") || !ObjectField(root, "metadata"))
                    throw new FormatException("timing JSON requires the successful schemaVersion 2 build timing shape");
            } else if (kind == "verdict") {
                if (!root.TryGetProperty("schema", out var schema) || !schema.TryGetInt32(out int version) || version != 1 ||
                    !root.TryGetProperty("status", out var status) || status.ValueKind != JsonValueKind.String ||
                    (status.GetString() != "PASS" && status.GetString() != "FAIL") ||
                    !root.TryGetProperty("reasons", out var reasons) || reasons.ValueKind != JsonValueKind.Array)
                    throw new FormatException("invalid evaluator JSON verdict shape");
                foreach (var reason in reasons.EnumerateArray()) {
                    if (reason.ValueKind != JsonValueKind.Object || !StringField(reason, "code") || !StringField(reason, "message"))
                        throw new FormatException("invalid evaluator JSON reason shape");
                }
            }
        }
        static bool StringField(JsonElement value, string name) => value.TryGetProperty(name, out var field) && field.ValueKind == JsonValueKind.String;
        static bool ObjectField(JsonElement value, string name) => value.TryGetProperty(name, out var field) && field.ValueKind == JsonValueKind.Object;
        static void Check(JsonElement value) {
            if (value.ValueKind == JsonValueKind.Object) {
                var names = new HashSet<string>(StringComparer.Ordinal);
                foreach (var property in value.EnumerateObject()) {
                    if (!names.Add(property.Name)) throw new FormatException("duplicate JSON property: " + property.Name);
                    Check(property.Value);
                }
            } else if (value.ValueKind == JsonValueKind.Array) {
                foreach (var element in value.EnumerateArray()) Check(element);
            } else if (value.ValueKind == JsonValueKind.String) {
                // JsonDocument decodes values lazily; force strict surrogate
                // validation before any later consumer can replace bad text.
                value.GetString();
            }
        }
    }
    public sealed class Capture : IDisposable {
        Stream pipe;
        readonly FileStream file;
        readonly long limit;
        public Task Completion { get; private set; } = Task.CompletedTask;
        public volatile bool Overflow;
        public volatile string Error;
        public long RetainedBytes { get; private set; }
        public double SpoolWriteMS { get; private set; }
        public Capture(Stream input, string path, long maxBytes) {
            pipe = input; limit = maxBytes;
            file = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.Read, 4096);
        }
        public void Start(Stream input = null) {
            if (input != null) pipe = input;
            Completion = Task.Factory.StartNew(Read, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
        }
        void Read() {
            var buffer = new byte[65536];
            try {
                int count;
                while ((count = pipe.Read(buffer, 0, buffer.Length)) != 0) {
                    int keep = (int)Math.Min(count, limit - RetainedBytes);
                    var clock = Stopwatch.StartNew();
                    if (keep != 0) { file.Write(buffer, 0, keep); file.Flush(); RetainedBytes += keep; }
                    SpoolWriteMS += clock.Elapsed.TotalMilliseconds;
                    if (keep != count) { Overflow = true; break; }
                }
            } catch (Exception ex) { Error = ex.Message; }
        }
        public void CloseEvidence() { try { file.Flush(); } finally { file.Dispose(); } }
        public void Dispose() { try { pipe?.Dispose(); } finally { file.Dispose(); } }
    }
    public sealed class Invocation : IDisposable {
        [StructLayout(LayoutKind.Sequential)] struct SecurityAttributes {
            public int Length; public IntPtr Descriptor; [MarshalAs(UnmanagedType.Bool)] public bool Inherit;
        }
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)] struct StartupInfo {
            public int Size; public string Reserved, Desktop, Title;
            public uint X, Y, XSize, YSize, XCountChars, YCountChars, FillAttribute, Flags;
            public ushort ShowWindow, ReservedSize;
            public IntPtr ReservedBytes, Input, Output, Error;
        }
        [StructLayout(LayoutKind.Sequential)] struct StartupInfoEx { public StartupInfo Info; public IntPtr Attributes; }
        [StructLayout(LayoutKind.Sequential)] struct ProcessInfo { public IntPtr Process, Thread; public uint ProcessId, ThreadId; }
        [StructLayout(LayoutKind.Sequential)] struct BasicLimits {
            public long ProcessTime, JobTime; public uint Flags;
            public UIntPtr MinimumWorkingSet, MaximumWorkingSet; public uint ActiveLimit;
            public UIntPtr Affinity; public uint PriorityClass, SchedulingClass;
        }
        [StructLayout(LayoutKind.Sequential)] struct IoCounters { public ulong ReadOps, WriteOps, OtherOps, ReadBytes, WriteBytes, OtherBytes; }
        [StructLayout(LayoutKind.Sequential)] struct ExtendedLimits {
            public BasicLimits Basic; public IoCounters Io;
            public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
        }
        [StructLayout(LayoutKind.Sequential)] struct Accounting {
            public long UserTime, KernelTime, PeriodUserTime, PeriodKernelTime;
            public uint PageFaults, TotalProcesses, ActiveProcesses, TerminatedProcesses;
        }
        [StructLayout(LayoutKind.Sequential)] struct CompletionPort { public IntPtr Key, Port; }
        [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateJobObject(IntPtr attributes, string name);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr job, int kind, ref ExtendedLimits info, uint size);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr job, int kind, ref CompletionPort info, uint size);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool QueryInformationJobObject(IntPtr job, int kind, out Accounting info, uint size, IntPtr returned);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool TerminateJobObject(IntPtr job, uint exitCode);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool CreatePipe(out IntPtr read, out IntPtr write, ref SecurityAttributes attributes, uint size);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetHandleInformation(IntPtr handle, uint mask, uint flags);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern IntPtr CreateFileW(string name, uint access, uint share, ref SecurityAttributes attributes, uint disposition, uint flags, IntPtr template);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool InitializeProcThreadAttributeList(IntPtr list, int count, int flags, ref UIntPtr size);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool UpdateProcThreadAttribute(IntPtr list, uint flags, UIntPtr attribute, IntPtr value, UIntPtr size, IntPtr previous, IntPtr returned);
        [DllImport("kernel32.dll")] static extern void DeleteProcThreadAttributeList(IntPtr list);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool CreateProcessW(string application, StringBuilder command, IntPtr processAttributes, IntPtr threadAttributes, bool inheritHandles, uint flags, IntPtr environment, string directory, ref StartupInfoEx startup, out ProcessInfo process);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetExitCodeProcess(IntPtr process, out uint code);
        [DllImport("kernel32.dll", SetLastError = true)] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
        [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateIoCompletionPort(IntPtr file, IntPtr existing, UIntPtr key, uint threads);
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetQueuedCompletionStatus(IntPtr port, out uint bytes, out UIntPtr key, out IntPtr overlapped, uint milliseconds);
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
        IntPtr job, port, process, outputWrite, errorWrite, input;
        Process portable;
        bool started, disposed;
        public static bool CleanupSafe { get; private set; } = true;
        public Capture Stdout { get; private set; }
        public Capture Stderr { get; private set; }
        public int ProcessId { get; private set; }
        static void Check(bool success) { if (!success) throw new Win32Exception(Marshal.GetLastWin32Error()); }
        static void Close(ref IntPtr handle) { if (handle != IntPtr.Zero) { CloseHandle(handle); handle = IntPtr.Zero; } }
        public Invocation(string stdout, string stderr, long maxBytes) {
            if (maxBytes < 1) throw new ArgumentOutOfRangeException(nameof(maxBytes));
            try {
                if (OperatingSystem.IsWindows()) {
                    job = CreateJobObject(IntPtr.Zero, null);
                    if (job == IntPtr.Zero) throw new Win32Exception();
                    var limits = new ExtendedLimits(); limits.Basic.Flags = 0x2000; // KILL_ON_JOB_CLOSE; never disabled.
                    Check(SetInformationJobObject(job, 9, ref limits, (uint)Marshal.SizeOf<ExtendedLimits>()));
                    port = CreateIoCompletionPort(new IntPtr(-1), IntPtr.Zero, UIntPtr.Zero, 1);
                    if (port == IntPtr.Zero) throw new Win32Exception();
                    var association = new CompletionPort { Key = new IntPtr(1), Port = port };
                    Check(SetInformationJobObject(job, 7, ref association, (uint)Marshal.SizeOf<CompletionPort>()));
                    Stdout = MakePipe(stdout, maxBytes, out outputWrite);
                    Stderr = MakePipe(stderr, maxBytes, out errorWrite);
                    var attributes = new SecurityAttributes { Length = Marshal.SizeOf<SecurityAttributes>(), Inherit = true };
                    input = CreateFileW("NUL", 0x80000000, 3, ref attributes, 3, 0, IntPtr.Zero);
                    if (input == new IntPtr(-1)) { input = IntPtr.Zero; throw new Win32Exception(); }
                } else {
                    // Portable ValidateOnly; scored runs require Windows.
                    portable = new Process();
                    Stdout = new Capture(null, stdout, maxBytes); Stderr = new Capture(null, stderr, maxBytes);
                }
            } catch { Dispose(); throw; }
        }
        static Capture MakePipe(string path, long limit, out IntPtr write) {
            var attributes = new SecurityAttributes { Length = Marshal.SizeOf<SecurityAttributes>(), Inherit = true };
            Check(CreatePipe(out IntPtr read, out write, ref attributes, 65536));
            var safe = new SafeFileHandle(read, true);
            try {
                Check(SetHandleInformation(read, 1, 0));
                var stream = new FileStream(safe, FileAccess.Read);
                try { return new Capture(stream, path, limit); }
                catch { stream.Dispose(); throw; }
            } catch { safe.Dispose(); Close(ref write); throw; }
        }
        // Windows argv backslash/quote rules, including empty arguments and
        // trailing backslashes. No shell or command substitution.
        static string Quote(string value) {
            var quoted = new StringBuilder("\""); int slashes = 0;
            foreach (char c in value) {
                if (c == '\\') { slashes++; continue; }
                if (c == '"') quoted.Append('\\', slashes * 2 + 1);
                else quoted.Append('\\', slashes);
                quoted.Append(c); slashes = 0;
            }
            return quoted.Append('\\', slashes * 2).Append('"').ToString();
        }
        public void Start(string executable, string[] arguments, string directory) {
            if (started) throw new InvalidOperationException("invocation already started");
            if (!OperatingSystem.IsWindows()) {
                var info = new ProcessStartInfo(executable) { WorkingDirectory = directory, UseShellExecute = false, RedirectStandardOutput = true, RedirectStandardError = true };
                foreach (string argument in arguments) info.ArgumentList.Add(argument);
                portable.StartInfo = info;
                if (!portable.Start()) throw new InvalidOperationException("could not start process");
                started = true; ProcessId = portable.Id;
                Stdout.Start(portable.StandardOutput.BaseStream); Stderr.Start(portable.StandardError.BaseStream); return;
            }
            IntPtr list = IntPtr.Zero, jobs = IntPtr.Zero, handles = IntPtr.Zero;
            bool initialized = false;
            try {
                UIntPtr size = UIntPtr.Zero;
                InitializeProcThreadAttributeList(IntPtr.Zero, 2, 0, ref size);
                list = Marshal.AllocHGlobal(checked((int)size.ToUInt64()));
                Check(InitializeProcThreadAttributeList(list, 2, 0, ref size)); initialized = true;
                jobs = Marshal.AllocHGlobal(IntPtr.Size); Marshal.WriteIntPtr(jobs, job);
                handles = Marshal.AllocHGlobal(IntPtr.Size * 3);
                Marshal.WriteIntPtr(handles, 0, input); Marshal.WriteIntPtr(handles, IntPtr.Size, outputWrite); Marshal.WriteIntPtr(handles, IntPtr.Size * 2, errorWrite);
                Check(UpdateProcThreadAttribute(list, 0, new UIntPtr(0x2000D), jobs, new UIntPtr((uint)IntPtr.Size), IntPtr.Zero, IntPtr.Zero));
                Check(UpdateProcThreadAttribute(list, 0, new UIntPtr(0x20002), handles, new UIntPtr((uint)(IntPtr.Size * 3)), IntPtr.Zero, IntPtr.Zero));
                var startup = new StartupInfoEx { Attributes = list };
                startup.Info.Size = Marshal.SizeOf<StartupInfoEx>(); startup.Info.Flags = 0x100;
                startup.Info.Input = input; startup.Info.Output = outputWrite; startup.Info.Error = errorWrite;
                var command = new StringBuilder(Quote(executable));
                foreach (string argument in arguments) command.Append(' ').Append(Quote(argument));
                Stdout.Start(); Stderr.Start();
                // EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT.
                // Null environment inherits unchanged; no suspended/attach or breakaway window.
                Check(CreateProcessW(executable, command, IntPtr.Zero, IntPtr.Zero, true, 0x08080400, IntPtr.Zero, directory, ref startup, out ProcessInfo created));
                process = created.Process; CloseHandle(created.Thread);
                ProcessId = (int)created.ProcessId; started = true;
            } finally {
                Close(ref outputWrite); Close(ref errorWrite); Close(ref input);
                if (initialized) DeleteProcThreadAttributeList(list);
                if (list != IntPtr.Zero) Marshal.FreeHGlobal(list);
                if (jobs != IntPtr.Zero) Marshal.FreeHGlobal(jobs);
                if (handles != IntPtr.Zero) Marshal.FreeHGlobal(handles);
            }
        }
        public bool RootExited {
            get {
                if (!started) return true;
                if (portable != null) return portable.HasExited;
                uint state = WaitForSingleObject(process, 0);
                if (state == 0xffffffff) throw new Win32Exception();
                return state == 0;
            }
        }
        public int? ExitCode {
            get {
                if (!started || !RootExited) return null;
                if (portable != null) return portable.ExitCode;
                Check(GetExitCodeProcess(process, out uint code)); return unchecked((int)code);
            }
        }
        public uint ActiveProcesses {
            get {
                if (portable != null) return RootExited ? 0u : 1u;
                if (job == IntPtr.Zero) return 0;
                Check(QueryInformationJobObject(job, 1, out Accounting accounting, (uint)Marshal.SizeOf<Accounting>(), IntPtr.Zero));
                return accounting.ActiveProcesses;
            }
        }
        public bool CapturesComplete => (Stdout == null || Stdout.Completion.IsCompleted) && (Stderr == null || Stderr.Completion.IsCompleted);
        public string CaptureFailure {
            get {
                if ((Stdout?.Overflow ?? false) || (Stderr?.Overflow ?? false)) return "capture overflow: per-stream byte limit exceeded; retained prefix only";
                if (Stdout?.Error != null || Stderr?.Error != null) return "capture retention failed: " + Stdout?.Error + "; " + Stderr?.Error;
                if ((Stdout?.Completion.IsFaulted ?? false) || (Stderr?.Completion.IsFaulted ?? false) ||
                    (Stdout?.Completion.IsCanceled ?? false) || (Stderr?.Completion.IsCanceled ?? false)) return "capture task failed or was cancelled; retention incomplete";
                return null;
            }
        }
        public void Wait(int milliseconds) {
            if (portable == null && ActiveProcesses != 0) {
                if (!GetQueuedCompletionStatus(port, out _, out _, out _, (uint)milliseconds) && Marshal.GetLastWin32Error() != 258) throw new Win32Exception();
            } else if (!CapturesComplete) {
                Task.WhenAll(Stdout?.Completion ?? Task.CompletedTask, Stderr?.Completion ?? Task.CompletedTask).Wait(milliseconds);
            } else if (portable != null && !RootExited) { portable.WaitForExit(milliseconds); }
        }
        public void CloseEvidence() { Stdout?.CloseEvidence(); Stderr?.CloseEvidence(); }
        public void TerminateAndJoin() {
            try {
                if (job != IntPtr.Zero && ActiveProcesses != 0) Check(TerminateJobObject(job, 1));
                if (portable != null && started && !portable.HasExited) portable.Kill(true);
                var timer = Stopwatch.StartNew();
                while (!RootExited || ActiveProcesses != 0 || !CapturesComplete) {
                    if (timer.ElapsedMilliseconds > 5000) throw new TimeoutException("owned invocation did not quiesce after termination; preserve work copies");
                    Wait(25);
                }
            } catch { CleanupSafe = false; throw; }
        }
        public void Dispose() {
            if (disposed) return; disposed = true;
            try { Close(ref outputWrite); Close(ref errorWrite); Close(ref input); TerminateAndJoin(); }
            finally {
                Close(ref process); Close(ref job); Close(ref port);
                try { Stdout?.Dispose(); } finally { Stderr?.Dispose(); portable?.Dispose(); }
            }
        }
    }
}
'@
}

function Read-StrictJson {
    param([string]$Path, [ValidateSet('object', 'public_result', 'timings', 'verdict')][string]$Kind = 'object', [switch]$PublicIdentity)

    Initialize-ProcessOwnership
    $stream = [IO.File]::OpenRead($Path)
    $reader = [IO.StreamReader]::new($stream, [Text.UTF8Encoding]::new($false, $true), $false)
    try { $raw = $reader.ReadToEnd() } finally { $reader.Dispose() }
    if ($PublicIdentity) {
        if ($Kind -ne 'public_result') { throw 'public identity requires the public_result shape' }
        return [RotorBench.StrictJson]::ReadPublicIdentity($raw)
    }
    [RotorBench.StrictJson]::Validate($raw, $Kind)
    $value = ConvertFrom-Json -InputObject $raw -AsHashtable -NoEnumerate -DateKind String
    if ($value -isnot [System.Collections.IDictionary]) { throw 'JSON result must be one object' }
    return $value
}

function Write-ProcessEvidence {
    param($Owner)

    # Chunks are already spooled. Only final flush/close is a tail write.
    $Owner.CloseEvidence()
}

function Invoke-ProcessWithTimeout {
    param(
        [string]$Executable,
        [string[]]$Arguments,
        [string]$WorkingDirectory,
        [TimeSpan]$Timeout,
        [string]$StdoutPath,
        [string]$StderrPath,
        [ref]$Ownership,
        [long]$CaptureLimit = $MaxCaptureBytes
    )

    Initialize-ProcessOwnership
    $owner = [RotorBench.Invocation]::new($StdoutPath, $StderrPath, $CaptureLimit)
    if ($null -ne $Ownership) { $Ownership.Value = $owner }
    $result = [ordered]@{ DurationMS = $null; ElapsedMS = $null; ExitCode = $null; Status = 'incomplete'; Error = $null; EvidencePersistenceMS = $null; CaptureLimitBytes = $CaptureLimit }
    $stopwatch = [Diagnostics.Stopwatch]::new()
    try {
        $stopwatch.Start()
        $owner.Start($Executable, $Arguments, $WorkingDirectory)
        while ($true) {
            if ($null -ne $owner.CaptureFailure) { throw $owner.CaptureFailure }
            if ($stopwatch.Elapsed -ge $Timeout) { $result.Status = 'timeout'; throw "owned process tree or redirected streams timed out after $Timeout" }
            if ($owner.RootExited -and $owner.ExitCode -ne 0) { $owner.TerminateAndJoin(); break }
            if ($owner.RootExited -and $owner.ActiveProcesses -eq 0 -and $owner.CapturesComplete) { break }
            $owner.Wait([int][Math]::Min(25, [Math]::Max(0, [Math]::Ceiling($Timeout.TotalMilliseconds - $stopwatch.Elapsed.TotalMilliseconds))))
        }
        $stopwatch.Stop()
        if ($null -ne $owner.CaptureFailure) { throw $owner.CaptureFailure }
        $result.Status = 'completed'
    } catch { $result.Error = $_.Exception.Message }
    finally {
        $stopwatch.Stop()
        $result.ElapsedMS = $stopwatch.Elapsed.TotalMilliseconds
        $result.DurationMS = [Math]::Max(1, [int64][Math]::Round($result.ElapsedMS))
        try {
            if ($result.Status -ne 'completed') { $owner.TerminateAndJoin() }
            $result.ExitCode = $owner.ExitCode
            $result.StdoutBytes = if ($null -ne $owner.Stdout) { $owner.Stdout.RetainedBytes } else { 0 }
            $result.StderrBytes = if ($null -ne $owner.Stderr) { $owner.Stderr.RetainedBytes } else { 0 }
            $result.CaptureOverflow = ($null -ne $owner.Stdout -and $owner.Stdout.Overflow) -or ($null -ne $owner.Stderr -and $owner.Stderr.Overflow)
            $result.CaptureSpoolWriteMS = $owner.Stdout.SpoolWriteMS + $owner.Stderr.SpoolWriteMS
            $persistence = [Diagnostics.Stopwatch]::StartNew()
            try { Write-ProcessEvidence $owner }
            catch { $result.Status = 'incomplete'; $result.Error += "; evidence persistence: $($_.Exception.Message)" }
            finally { $persistence.Stop(); $result.EvidencePersistenceMS = $persistence.Elapsed.TotalMilliseconds }
        } finally { if ($null -eq $Ownership) { $owner.Dispose() } }
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
        diagnostics_digest_format = 'sha256-diagnostics-v2'
    }
    Add-InvocationRecord $JournalPath $entry
    $result = $null
    $owner = $null
    $validationCompleted = $false
    $entry.error = $null
    try {
        $result = Invoke-ProcessWithTimeout $Executable (@('build', '--project', $WorkCopy, '--json') + $ExtraArguments) $WorkCopy $Timeout $entry.stdout $entry.stderr -Ownership ([ref]$owner)
        if ($result.Status -ne 'completed') { throw $result.Error }
        try { $publicResult = Read-StrictJson $entry.stdout -Kind public_result -PublicIdentity }
        catch { throw "invalid public JSON: $($_.Exception.Message)" }
        if ($result.ExitCode -ne 0 -or -not $publicResult.Ok) { throw "build failed (exit_code=$($result.ExitCode), ok=$($publicResult.Ok))" }
        $result.DiagnosticsDigest = $publicResult.DiagnosticsDigest
        $result.OutputTreeDigest = Get-TreeDigest $WorkCopy -OutputStateOnly
        $validationCompleted = $true
    } catch {
        $entry.error = $_.Exception.Message
        if ($null -ne $result) {
            if ($result.Status -eq 'completed') { $result.Status = 'incomplete' }
            $result.Error = $_.Exception.Message
        }
        throw
    } finally {
        try { if ($null -ne $owner) { $owner.Dispose() } }
        catch { $validationCompleted = $false; $entry.error += "; ownership cleanup: $($_.Exception.Message)"; throw }
        finally {
            if (-not $validationCompleted) {
                if (-not $entry.error) { $entry.error = 'validation interrupted or did not complete' }
                if ($null -ne $result -and $result.Status -eq 'completed') { $result.Status = 'incomplete'; $result.Error = $entry.error }
            }
            $entry.event = 'finished'
            $entry.at_utc = [DateTime]::UtcNow.ToString('o')
            $entry.validation_completed = $validationCompleted
            $entry.status = if ($validationCompleted) { 'completed' } else { 'incomplete' }
            $entry.result = $result
            Add-InvocationRecord $JournalPath $entry
        }
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
    $verdict = Read-StrictJson $VerdictPath -Kind verdict
    if ($verdict['schema'] -ne 1 -or $verdict['status'] -notin @('PASS', 'FAIL') -or (($result.ExitCode -eq 0) -ne ($verdict['status'] -eq 'PASS'))) {
        throw 'perfcompare exit code and verdict are inconsistent'
    }
    return [ordered]@{ ExitCode = $result.ExitCode; Status = $verdict['status']; Output = [IO.File]::ReadAllText("$VerdictPath.stderr.txt") }
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
            $package = Read-StrictJson $path
            $packages[$name] = [ordered]@{ version = $package['version']; manifest_sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash }
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
    Initialize-ProcessOwnership
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
    $evaluationReady = $false
    $cleanupCompleted = $false
    $run = [ordered]@{
        schema = 1; status = 'incomplete'; started_utc = [DateTime]::UtcNow.ToString('o'); finished_utc = $null
        error = $null; before = $null; after = $null; evaluation = $null
        invocation_journal = $journalPath
        tree_digest_format = 'sha256-tree-v2'
        diagnostics_digest_format = 'sha256-diagnostics-v2'
        measurement_boundary = 'owned-job-quiescence-and-spooled-stream-eof-v2'
        capture_limit_bytes_per_stream = $MaxCaptureBytes
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
        [void](Read-StrictJson $timingPath -Kind timings)
        $run.after = Get-InputIdentity $trackedWorktrees $go
        Assert-InputIdentityEqual $run.before $run.after
        Assert-EnvironmentEqual $environment (Get-EnvironmentSnapshot)
        $manifest = [ordered]@{
            schema = 1
            tree_digest_format = 'sha256-tree-v2'
            measurement_boundary = 'owned-job-quiescence-and-spooled-stream-eof-v2'
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
            $verdict = Read-StrictJson $verdictPath -Kind verdict
            if (@($verdict['reasons'] | Where-Object { $_['code'] -eq 'invalid_manifest' }).Count -gt 0) {
                throw "perfcompare rejected invalid evidence: $($evaluation.Output)"
            }
        }
        $evaluationReady = $true
    } catch {
        $run.error = $_.Exception.Message
        throw
    } finally {
        try {
            if (-not $evaluationReady -and $null -ne $run.before) {
                try { $run.after = Get-InputIdentity $trackedWorktrees $go }
                catch { $run.error += "; final identity unavailable: $($_.Exception.Message)"; $run.status = 'incomplete' }
            }
        } finally {
            if ($null -eq $oldSidecarTimeout) {
                Remove-Item Env:ROTOR_SIDECAR_TIMEOUT -ErrorAction SilentlyContinue
            } else {
                $env:ROTOR_SIDECAR_TIMEOUT = $oldSidecarTimeout
            }
            try {
                if (-not [RotorBench.Invocation]::CleanupSafe) { throw 'owned process cleanup did not quiesce; preserve work copies' }
                Remove-TestOwnedWorkCopies $evidencePath $workRoot
                $cleanupCompleted = $true
            }
            catch {
                $run.cleanup_error = $_.Exception.Message
                $run.error = if ($run.error) { $run.error + "; ownership-safe cleanup failed: $($_.Exception.Message)" } else { "ownership-safe cleanup failed: $($_.Exception.Message)" }
                throw
            } finally {
                # Cancellation can bypass catch; keep both completion gates false
                # until their entire operations have returned successfully.
                if (-not $evaluationReady -and -not $run.error) { $run.error = 'run interrupted before evaluation completed' }
                if (-not $cleanupCompleted -and -not $run.error) { $run.error = 'cleanup interrupted or did not complete' }
                $run.status = 'incomplete'
                $run.finished_utc = [DateTime]::UtcNow.ToString('o')
                Write-RunStatus $runPath $run
            }
        }
    }
    # Publish only after identity, evaluation, ownership and cleanup have all
    # completed. Roll back the filename if publication itself is interrupted.
    $publicationCompleted = $false
    $publicationFailure = $null
    if (-not $evaluationReady -or -not $cleanupCompleted) { throw 'evaluation or cleanup did not complete' }
    try {
        $run.status = 'evaluated'
        if ($evaluation.ExitCode -ne 0) { $run.error = "perfcompare rejected benchmark evidence: $($evaluation.Output)" }
        Write-RunStatus $runPath $run
        [IO.File]::Move($pendingManifest, $manifestPath)
        $publicationCompleted = $true
    } catch {
        $publicationFailure = $_
        $detail = "manifest publication failed: $($_.Exception.Message)"
        $run.error = if ($run.error) { "$($run.error); $detail" } else { $detail }
        throw
    } finally {
        if (-not $publicationCompleted) {
            $run.status = 'incomplete'
            if ($null -eq $publicationFailure) {
                $detail = 'manifest publication interrupted or did not complete'
                $run.error = if ($run.error) { "$($run.error); $detail" } else { $detail }
            }
            try {
                if (Test-Path -LiteralPath $manifestPath) { [IO.File]::Move($manifestPath, $pendingManifest) }
            } catch {
                $run.error += "; manifest rollback failed: $($_.Exception.Message)"
                if ($null -ne $publicationFailure) { $publicationFailure.Exception.Data['rollback_error'] = $_.Exception.Message }
            }
            try { Write-RunStatus $runPath $run }
            catch {
                # Preserve the original publication exception or cancellation.
                # A failed status write cannot guarantee on-disk error retention.
                if ($null -ne $publicationFailure) { $publicationFailure.Exception.Data['status_retention_error'] = $_.Exception.Message }
            }
        }
    }
    if ($evaluation.ExitCode -ne 0) { throw $run.error }
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
