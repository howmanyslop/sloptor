import { accessSync, constants, statSync } from "node:fs";
import nodePath from "node:path";
import { env, platform as processPlatform } from "node:process";

export interface BinaryEnvironment {
	readonly environment: Readonly<Record<string, string | undefined>>;
	readonly isExecutable: (file: string) => boolean;
	readonly platform: NodeJS.Platform;
}

export interface BinaryQuery {
	/** `sloptor.path`, already trimmed. */
	readonly configuredPath: string | undefined;
	/** What a relative `sloptor.path` is relative to. */
	readonly firstWorkspaceFolder: string | undefined;
	/** Where the `node_modules/.bin` walk starts. */
	readonly searchFrom: string | undefined;
}

const LOCAL_NAME = "rotor";
const PATH_NAMES: ReadonlyArray<string> = ["sloptor", "rotor"];
const POSIX_EXTENSIONS: ReadonlyArray<string> = [""];
const WINDOWS_LOCAL_EXTENSIONS: ReadonlyArray<string> = [".exe", ".cmd"];
const DEFAULT_PATHEXT = ".COM;.EXE;.BAT;.CMD";

function getPathModule(platform: NodeJS.Platform): typeof nodePath.posix {
	return platform === "win32" ? nodePath.win32 : nodePath.posix;
}

function readVariable(environment: BinaryEnvironment, name: string): string | undefined {
	if (environment.platform !== "win32") return environment.environment[name];

	// Windows variable names ignore case, and a plain env object does not.
	for (const [key, value] of Object.entries(environment.environment)) {
		if (key.toUpperCase() === name) return value;
	}

	return undefined;
}

function splitList(value: string | undefined, delimiter: string): Array<string> {
	if (value === undefined) {
		return [];
	}

	const result = new Array<string>();
	let size = 0;
	for (const entry of value.split(delimiter)) {
		const trimmed = entry.trim();
		const unquoted =
			trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"') ? trimmed.slice(1, -1) : trimmed;

		if (unquoted.length === 0) continue;

		result[size] = unquoted;
		size += 1;
	}

	return result;
}

function findWithExtensions(
	environment: BinaryEnvironment,
	base: string,
	extensions: ReadonlyArray<string>,
): string | undefined {
	for (const extension of extensions) {
		const candidate = base + extension;
		if (environment.isExecutable(candidate)) return candidate;
	}

	return undefined;
}

function findConfigured(
	environment: BinaryEnvironment,
	query: BinaryQuery,
	configuredPath: string,
	pathExtensions: ReadonlyArray<string>,
): string | undefined {
	const paths = getPathModule(environment.platform);
	let absolute = configuredPath;
	if (!paths.isAbsolute(configuredPath)) {
		if (query.firstWorkspaceFolder === undefined) return undefined;
		absolute = paths.resolve(query.firstWorkspaceFolder, configuredPath);
	}

	if (environment.platform !== "win32") return findWithExtensions(environment, absolute, POSIX_EXTENSIONS);
	return findWithExtensions(environment, absolute, ["", ...pathExtensions]);
}

function findLocal(environment: BinaryEnvironment, searchFrom: string): string | undefined {
	const paths = getPathModule(environment.platform);
	const extensions = environment.platform === "win32" ? WINDOWS_LOCAL_EXTENSIONS : POSIX_EXTENSIONS;

	let directory = paths.resolve(searchFrom);
	for (;;) {
		const found = findWithExtensions(
			environment,
			paths.join(directory, "node_modules", ".bin", LOCAL_NAME),
			extensions,
		);

		if (found !== undefined) return found;

		const parent = paths.dirname(directory);
		if (parent === directory) return undefined;
		directory = parent;
	}
}

function findOnPath(environment: BinaryEnvironment, pathExtensions: ReadonlyArray<string>): string | undefined {
	const isWindows = environment.platform === "win32";
	const paths = getPathModule(environment.platform);
	const directories = splitList(readVariable(environment, "PATH"), paths.delimiter);
	const extensions = isWindows ? pathExtensions : POSIX_EXTENSIONS;

	for (const name of PATH_NAMES) {
		for (const directory of directories) {
			const found = findWithExtensions(environment, paths.join(directory, name), extensions);
			if (found !== undefined) return found;
		}
	}

	return undefined;
}

/**
 * Finds the sloptor executable. `sloptor.path` wins when it is set, and a set
 * path that does not exist finds nothing rather than quietly running something
 * else. Otherwise the npm package's `node_modules/.bin/rotor` nearest
 * `searchFrom`, then `sloptor` and `rotor` on `PATH`.
 *
 * @param query - The configured path and the folders to resolve against.
 * @param environment - The variables, platform and file check to use.
 * @returns The absolute path of the executable, or `undefined` for none.
 */
export function resolveBinary(query: BinaryQuery, environment: BinaryEnvironment): string | undefined {
	const pathExtensions =
		environment.platform === "win32" ? splitList(readVariable(environment, "PATHEXT") ?? DEFAULT_PATHEXT, ";") : [];

	if (query.configuredPath !== undefined) {
		return findConfigured(environment, query, query.configuredPath, pathExtensions);
	}

	if (query.searchFrom !== undefined) {
		const local = findLocal(environment, query.searchFrom);
		if (local !== undefined) return local;
	}

	return findOnPath(environment, pathExtensions);
}

/**
 * Whether `file` is a regular file this process may execute. Windows has no
 * execute bit, so there a file is enough.
 *
 * @param file - The absolute path to check.
 * @param platform - The platform the check runs on.
 * @returns `true` for an executable file.
 */
export function isExecutableFile(file: string, platform: NodeJS.Platform): boolean {
	try {
		const stats = statSync(file, { throwIfNoEntry: false });
		if (stats?.isFile() !== true) return false;
		if (platform === "win32") return true;

		accessSync(file, constants.X_OK);
		return true;
	} catch {
		return false;
	}
}

/**
 * The real process environment for {@link resolveBinary}.
 *
 * @returns The environment of this process.
 */
export function createProcessEnvironment(): BinaryEnvironment {
	return {
		environment: env,
		isExecutable(file: string): boolean {
			return isExecutableFile(file, processPlatform);
		},
		platform: processPlatform,
	};
}
