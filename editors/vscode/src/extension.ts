import { commands, window, workspace } from "vscode";

import { registerColorProvider } from "./color/color-provider";
import { Compiler } from "./compiler";
import { LanguageServer } from "./language-client";
import { openOutputAsync } from "./output";
import { readSettings } from "./settings";

import type { ConfigurationChangeEvent, Disposable, ExtensionContext } from "vscode";

import type { JsonValue } from "./json";
import type { Settings } from "./settings";

let activeServer: LanguageServer | undefined;

function loadSettings(): Settings {
	const configuration = workspace.getConfiguration("sloptor");
	return readSettings((key) => configuration.get<JsonValue>(key));
}

function needsRestart(previous: Settings, next: Settings): boolean {
	return previous.path !== next.path || previous.typeErrors !== next.typeErrors;
}

/**
 * Wires the language server, the compiler commands, the status bar button and
 * the color picker. The server starts in the background; activation does not
 * wait for it.
 *
 * @param context - The extension context that owns every disposable.
 */
export function activate(context: ExtensionContext): void {
	let settings = loadSettings();

	function getSettings(): Settings {
		return settings;
	}

	const outputChannel = window.createOutputChannel("sloptor", { log: true });
	const server = new LanguageServer(outputChannel, getSettings);
	const compiler = new Compiler(getSettings);
	activeServer = server;

	let colorProvider: Disposable | undefined;

	function updateColorProvider(): void {
		if (settings.colorPickerEnabled) {
			colorProvider ??= registerColorProvider(() => settings.colorPrecision);
			return;
		}

		colorProvider?.dispose();
		colorProvider = undefined;
	}

	function onConfigurationChanged(event: ConfigurationChangeEvent): void {
		if (!event.affectsConfiguration("sloptor")) return;

		const previous = settings;
		settings = loadSettings();
		if (needsRestart(previous, settings)) {
			server.restartAsync().catch(() => undefined);
		}

		if (previous.colorPickerEnabled !== settings.colorPickerEnabled) {
			updateColorProvider();
		}

		if (previous.statusBarEnabled !== settings.statusBarEnabled) {
			compiler.refresh();
		}
	}

	context.subscriptions.push(
		outputChannel,
		compiler,
		{
			dispose(): void {
				colorProvider?.dispose();
			},
		},
		commands.registerCommand("sloptor.openOutput", async () => openOutputAsync(server, settings)),
		commands.registerCommand("sloptor.startCompiler", async () => compiler.startAsync()),
		commands.registerCommand("sloptor.stopCompiler", async () => compiler.stopAsync()),
		commands.registerCommand("sloptor.toggleCompiler", async () => compiler.toggleAsync()),
		commands.registerCommand("sloptor.restartServer", async () => server.restartAsync()),
		workspace.onDidChangeConfiguration(onConfigurationChanged),
		window.onDidChangeActiveTextEditor(() => {
			compiler.refresh();
		}),
	);

	updateColorProvider();
	compiler.refresh();
	server.startAsync().catch(() => undefined);
}

/**
 * Stops the language server. The compiler and everything else go with the
 * context's subscriptions.
 *
 * @returns Resolves once the server has exited, or `undefined` when it never
 *   started.
 */
// oxlint-disable-next-line small-rules/starts-with-verb -- VS Code looks the export up by this name.
export function deactivate(): Promise<void> | undefined {
	const server = activeServer;
	activeServer = undefined;
	return server?.stopAsync();
}
