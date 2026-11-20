/** The part of a VS Code `WorkspaceFolder` the compiler commands read. */
export interface CompilerFolder {
	readonly name: string;
	readonly uri: { readonly scheme: string };
}

/**
 * The active editor, as far as picking a folder goes.
 *
 * @template Folder - The workspace folder type.
 */
export interface ActiveEditor<Folder extends CompilerFolder> {
	/** `undefined` when the file is outside every folder. */
	readonly folder?: Folder | undefined;
}

export interface CompilerStatus {
	readonly text: string;
	readonly tooltip: string;
}

export interface CompilerStatusOptions {
	/** The folder from {@link pickCompilerFolder}. */
	readonly folder?: CompilerFolder | undefined;
	readonly isEnabled: boolean;
	readonly isRunning: boolean;
	/** The active editor's language, if there is one. */
	readonly languageId?: string | undefined;
}

const TYPESCRIPT_LANGUAGES: ReadonlySet<string> = new Set(["typescript", "typescriptreact"]);

function isLocalFolder<Folder extends CompilerFolder>(folder?: Folder): folder is Folder {
	return folder?.uri.scheme === "file";
}

/**
 * The folder the compiler commands act on: the active file's when there is an
 * active editor, otherwise the only folder. Only a local folder can run a
 * process.
 *
 * @template Folder - The workspace folder type.
 * @param folders - Every workspace folder.
 * @param activeEditor - The active editor, if there is one.
 * @returns The folder, or `undefined` when there is none to pick.
 */
export function pickCompilerFolder<Folder extends CompilerFolder>(
	folders: ReadonlyArray<Folder>,
	activeEditor?: ActiveEditor<Folder>,
): Folder | undefined {
	if (activeEditor !== undefined) return isLocalFolder(activeEditor.folder) ? activeEditor.folder : undefined;

	const only = folders.length === 1 ? folders[0] : undefined;
	return isLocalFolder(only) ? only : undefined;
}

/**
 * The status bar button for the active editor. It shows only on a TypeScript
 * file in a folder the compiler can run in.
 *
 * @param options - What the button depends on.
 * @returns The button, or `undefined` when it should be hidden.
 */
export function getCompilerStatus({
	folder,
	isEnabled,
	isRunning,
	languageId,
}: CompilerStatusOptions): CompilerStatus | undefined {
	if (!isEnabled || folder === undefined || languageId === undefined || !TYPESCRIPT_LANGUAGES.has(languageId)) {
		return undefined;
	}

	return isRunning
		? { text: "$(debug-stop) sloptor", tooltip: `Stop the sloptor compiler in ${folder.name}` }
		: { text: "$(debug-start) sloptor", tooltip: `Start the sloptor compiler in ${folder.name}` };
}
