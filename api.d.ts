export type SloptorClientErrorCode =
	| "BUILD_CANCELLED"
	| "CAPABILITY_MISMATCH"
	| "EXECUTABLE_NOT_FOUND"
	| "INVALID_REQUEST"
	| "PROTOCOL_ERROR"
	| "PROTOCOL_VERSION_MISMATCH"
	| "SERVER_ERROR"
	| "SERVER_EXITED"
	| "SESSION_DISPOSED"
	| "SESSION_TERMINAL"
	| "SPAWN_FAILED"
	| "UNSUPPORTED_PLATFORM"
	| "VERSION_MISMATCH";

export declare class SloptorClientError extends Error {
	readonly code: SloptorClientErrorCode | (string & {});
}

export interface BuildDiagnostic {
	readonly code?: string;
	readonly col: number;
	readonly file: string;
	readonly line: number;
	readonly message: string;
	readonly severity: "error" | "warning";
}

export interface BuildResult {
	readonly diagnostics: readonly BuildDiagnostic[];
	readonly durationMs: number;
	readonly files: number;
	readonly ok: boolean;
	readonly outputs: readonly string[];
	readonly projects: readonly ProjectBuildResult[];
	readonly telemetry: BuildTelemetry;
}

export interface BuildTelemetry {
	readonly emittedProjects: number;
	readonly satisfiedProjects: number;
	readonly scheduledProjects: number;
	readonly selectedProjects: number;
	readonly transformedProjects: number;
}

export type ProjectBuildStatus = "success" | "no-change" | "satisfied" | "blocked" | "failed";

export interface ProjectBuildResult {
	readonly blockers: readonly string[];
	readonly config: string;
	readonly diagnostics: readonly BuildDiagnostic[];
	readonly outputCount: number;
	readonly outputs: readonly string[];
	readonly status: ProjectBuildStatus;
	readonly timings: ProjectBuildTimings;
}

export interface ProjectBuildTimings {
	readonly counts: BuildTimingCounts;
	readonly durationMs: number;
	readonly stages: BuildTimingStages;
}

export interface BuildTimingStages {
	readonly cleanupMs: number;
	readonly compiledOutputWritesMs: number;
	readonly declarationEmitMs: number;
	readonly declarationEmitWritesMs: number;
	readonly includeCopyMs: number;
	readonly incrementalManifestMs: number;
	readonly incrementalSelectionMs: number;
	readonly initialProgramMs: number;
	readonly nativeTransformRenderMs: number;
	readonly nonCompiledCopyMs: number;
	readonly overlayProgramMs: number;
	readonly persistenceMs: number;
	readonly projectContextMs: number;
	readonly semanticDiagnosticsMs: number;
	readonly sidecarPreparationMs: number;
	readonly sidecarResponseDecodeMs: number;
	readonly sidecarRoundTripMs: number;
	readonly sidecarSessionWaitMs: number;
}

export interface BuildTimingCounts {
	readonly actualWrites: number;
	readonly effectiveWriteWorkers: number;
	readonly emittedEntries: number;
	readonly emittedProjects: number;
	readonly hashSkips: number;
	readonly nodeCPUSystemUs?: number;
	readonly nodeCPUUserUs?: number;
	readonly nodeWallMs?: number;
	readonly parseCacheHits?: number;
	readonly parseCacheMisses?: number;
	readonly satisfiedProjects: number;
	readonly scheduledDeclarationWrites: number;
	readonly scheduledProjects: number;
	readonly scheduledSourceMapWrites: number;
	readonly selectedProjects: number;
	readonly selectedSources: number;
	readonly sidecarChangedFiles?: number;
	readonly sidecarRequestBytes?: number;
	readonly sidecarResponseBytes?: number;
	readonly sidecarRestarts?: number;
	readonly sidecarSourceReads?: number;
	readonly sidecarSpawns?: number;
	readonly sidecarStats?: number;
	readonly totalSources: number;
	readonly transformedProjects: number;
	readonly uniquePreparedDirectories: number;
}

interface BuildRequestOptions {
	readonly signal?: AbortSignal;
}

type BuildOwnership =
	| { readonly selected?: never; readonly satisfied?: never }
	| { readonly selected: readonly string[]; readonly satisfied: readonly string[] };

export type BuildRequest = BuildRequestOptions &
	(
		| { readonly project: string; readonly roots?: never }
		| { readonly project?: never; readonly roots: readonly string[] }
	) &
	BuildOwnership;

export interface BuildSession extends AsyncDisposable {
	build(request: BuildRequest): Promise<BuildResult>;
	dispose(): Promise<void>;
}

export interface BuildSessionOptions {
	readonly executable?: string;
	/** Arguments inserted before the private api-server command. Primarily useful for process wrappers. */
	readonly executableArgs?: readonly string[];
}

export declare function createBuildSession(options?: BuildSessionOptions): BuildSession;
