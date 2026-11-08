export interface SpawnCommand {
	readonly arguments: Array<string>;
	readonly command: string;
	/**
	 * Only a Windows batch file needs one, and then `command` is the whole
	 * escaped line.
	 */
	readonly shell: boolean;
}

const BATCH_FILE = /\.(?:bat|cmd)$/iu;
const NPM_SHIM = /node_modules[/\\]\.bin[/\\][^/\\]+\.cmd$/iu;
const CMD_META_CHARACTERS = /[ !"%&()*,;<>?[\]^`|]/gu;

function escapeMetaCharacters(text: string): string {
	return text.replaceAll(CMD_META_CHARACTERS, "^$&");
}

/**
 * CommandLineToArgvW quoting: backslashes double only before a quote or the
 * closing quote.
 *
 * @param argument - One argument.
 * @returns The argument in double quotes.
 */
function quoteArgument(argument: string): string {
	let result = '"';
	let backslashes = 0;
	for (const character of argument) {
		if (character === "\\") {
			backslashes += 1;
			continue;
		}

		result +=
			character === '"' ? `${"\\".repeat(backslashes * 2 + 1)}"` : `${"\\".repeat(backslashes)}${character}`;
		backslashes = 0;
	}

	return `${result}${"\\".repeat(backslashes * 2)}"`;
}

function escapeArgument(argument: string, isNpmShim: boolean): string {
	const escaped = escapeMetaCharacters(quoteArgument(argument));
	// An npm shim hands `%*` to a second cmd parse, which eats more carets.
	return isNpmShim ? escapeMetaCharacters(escaped) : escaped;
}

/**
 * Builds the spawn for `binary` with `commandArguments`. Node refuses to spawn
 * a `.cmd` or `.bat` without a shell, so on Windows those become one cmd line
 * with every argument quoted and escaped; everything else is a plain argv with
 * no shell.
 *
 * @param binary - The absolute path of the executable.
 * @param commandArguments - The arguments, unquoted.
 * @param platform - The platform the process runs on.
 * @returns What to pass to `child_process.spawn` or a language client
 *   `Executable`.
 */
export function createSpawnCommand(
	binary: string,
	commandArguments: ReadonlyArray<string>,
	platform: NodeJS.Platform,
): SpawnCommand {
	if (platform !== "win32" || !BATCH_FILE.test(binary)) {
		return { arguments: [...commandArguments], command: binary, shell: false };
	}

	const isNpmShim = NPM_SHIM.test(binary);
	let line = escapeMetaCharacters(binary);
	for (const argument of commandArguments) {
		line += ` ${escapeArgument(argument, isNpmShim)}`;
	}

	return { arguments: [], command: line, shell: true };
}
