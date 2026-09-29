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
}

export interface BuildRequest {
	readonly project: string;
	readonly signal?: AbortSignal;
}

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
