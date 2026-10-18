export type SloptorClientErrorCode =
	| "BUILD_CANCELLED"
	| "BUILD_FAILED"
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

// oxlint-disable-next-line unicorn/custom-error-definition -- no.
export declare class SloptorClientError extends Error {
	// oxlint-disable-next-line typescript/ban-types -- no.
	public readonly code: SloptorClientErrorCode | (string & {});
}

export interface BuildDiagnostic {
	readonly code?: string | undefined;
	readonly col: number;
	readonly file: string;
	readonly line: number;
	readonly message: string;
	readonly severity: "error" | "warning";
}

export interface BuildResult {
	readonly diagnostics: ReadonlyArray<BuildDiagnostic>;
	readonly durationMs: number;
	readonly files: number;
	readonly ok: boolean;
	readonly outputs: ReadonlyArray<string>;
	readonly projects: ReadonlyArray<ProjectBuildResult>;
	readonly telemetry: BuildTelemetry;
}

export interface BuildTelemetry {
	readonly emittedProjects: number;
	readonly satisfiedProjects: number;
	readonly scheduledProjects: number;
	readonly selectedProjects: number;
	readonly transformedProjects: number;
}

export type ProjectBuildStatus = "blocked" | "failed" | "no-change" | "satisfied" | "success";

export interface ProjectBuildResult {
	readonly blockers: ReadonlyArray<string>;
	readonly config: string;
	readonly diagnostics: ReadonlyArray<BuildDiagnostic>;
	readonly outputCount: number;
	readonly outputs: ReadonlyArray<string>;
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
	readonly nodeCpuSystemUs?: number | undefined;
	readonly nodeCpuUserUs?: number | undefined;
	readonly nodeWallMs?: number | undefined;
	readonly parseCacheHits?: number | undefined;
	readonly parseCacheMisses?: number | undefined;
	readonly satisfiedProjects: number;
	readonly scheduledDeclarationWrites: number;
	readonly scheduledProjects: number;
	readonly scheduledSourceMapWrites: number;
	readonly selectedProjects: number;
	readonly selectedSources: number;
	readonly sidecarChangedFiles?: number | undefined;
	readonly sidecarRequestBytes?: number | undefined;
	readonly sidecarResponseBytes?: number | undefined;
	readonly sidecarRestarts?: number | undefined;
	readonly sidecarSourceReads?: number | undefined;
	readonly sidecarSpawns?: number | undefined;
	readonly sidecarStats?: number | undefined;
	readonly totalSources: number;
	readonly transformedProjects: number;
	readonly uniquePreparedDirectories: number;
}

interface BuildRequestOptions {
	readonly signal?: AbortSignal;
}

type BuildOwnership =
	| { readonly satisfied: ReadonlyArray<string>; readonly selected: ReadonlyArray<string> }
	| { readonly satisfied?: never; readonly selected?: never };

export type BuildRequest = BuildOwnership &
	BuildRequestOptions &
	(
		| { readonly project: string; readonly roots?: never }
		| { readonly project?: never; readonly roots: ReadonlyArray<string> }
	);

export interface BuildSession extends AsyncDisposable {
	build(request: BuildRequest): Promise<BuildResult>;
	dispose(): Promise<void>;
}

export interface BuildSessionOptions {
	readonly executable?: string | undefined;
	/**
	 * Arguments inserted before the private api-server command. Primarily useful
	 * for process wrappers.
	 */
	readonly executableArgs?: ReadonlyArray<string> | undefined;
}

export declare function createBuildSession(options?: BuildSessionOptions): BuildSession;
