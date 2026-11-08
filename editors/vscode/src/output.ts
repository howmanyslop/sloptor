import { Uri, ViewColumn, window, workspace } from "vscode";

import { describeOutputPathError, OUTPUT_PATH_METHOD, parseOutputPathAnswer } from "./output-path";

import type { LanguageServer } from "./language-client";
import type { OutputPathAnswer } from "./output-path";
import type { Settings } from "./settings";

async function existsAsync(uri: Uri): Promise<boolean> {
	try {
		await workspace.fs.stat(uri);
		return true;
	} catch {
		return false;
	}
}

/**
 * Open Output: asks the server where the active file compiles to and opens
 * that file.
 *
 * @param server - The language server to ask.
 * @param settings - The current settings.
 * @returns Resolves once the output is open or the user has been told why
 *   not.
 */
export async function openOutputAsync(server: LanguageServer, settings: Settings): Promise<void> {
	const editor = window.activeTextEditor;
	if (editor === undefined) {
		await window.showErrorMessage("Open Output needs an open TypeScript file.");
		return;
	}

	let answer: OutputPathAnswer;

	try {
		const result = await server.sendRequestAsync(OUTPUT_PATH_METHOD, {
			textDocument: { uri: editor.document.uri.toString() },
		});

		answer = parseOutputPathAnswer(result);
	} catch (error) {
		await window.showErrorMessage(describeOutputPathError(error));
		return;
	}

	if (answer.kind === "not-typescript") {
		await window.showErrorMessage("Open Output only works on a TypeScript source file.");
		return;
	}

	if (answer.kind === "malformed") {
		await window.showErrorMessage("sloptor/outputPath answered with something that is not a path.");
		return;
	}

	const output = Uri.file(answer.path);
	if (!(await existsAsync(output))) {
		await window.showWarningMessage(`${answer.path} is not compiled yet — start the compiler.`);
		return;
	}

	const viewColumn = settings.openOutputToSide ? ViewColumn.Beside : ViewColumn.Active;
	await window.showTextDocument(output, { viewColumn });
}
