import { once } from "node:events";
import { exec } from "tinyexec";
import { commands, window, workspace } from "vscode";
import { LanguageClient } from "vscode-languageclient/node";

import { createProcessEnvironment, resolveBinary } from "./binary";

import type { ChildProcess } from "node:child_process";
import type { Disposable, LogOutputChannel } from "vscode";
import type { DocumentSelector } from "vscode-languageclient/node";

import type { JsonObject, JsonValue } from "./json";
import type { Settings } from "./settings";

const CLIENT_ID = "sloptor";
const OPEN_SETTINGS = "Open Settings";
const SERVER_ARGUMENTS: ReadonlyArray<string> = ["lsp", "--stdio"];

const DOCUMENT_SELECTOR: DocumentSelector = [
	{ language: "typescript", scheme: "file" },
	{ language: "typescriptreact", scheme: "file" },
];

/**
 * Starts `sloptor lsp`. The client reads and writes its stdio and kills it if
 * it outlives a shutdown.
 *
 * @param binary - The sloptor executable.
 * @returns The running server.
 * @rejects When the binary cannot be spawned.
 */
async function spawnServerAsync(binary: string): Promise<ChildProcess> {
	const server = exec(binary, SERVER_ARGUMENTS).process;
	if (server === undefined) {
		throw new Error(`Couldn't start ${binary}`);
	}

	// Rejects on `error`, so a binary that can't run fails the start instead of
	// leaving the client waiting on a dead pipe.
	await once(server, "spawn");
	return server;
}

/**
 * The first workspace folder, which a relative `sloptor.path` is relative to.
 *
 * @returns Its file system path, or `undefined` without a local folder.
 */
export function getFirstFolderPath(): string | undefined {
	const folder = workspace.workspaceFolders?.[0];
	if (folder?.uri.scheme !== "file") return undefined;
	return folder.uri.fsPath;
}

/**
 * Finds the sloptor executable for a folder with the current settings.
 *
 * @param settings - The current settings.
 * @param searchFrom - Where the `node_modules/.bin` walk starts.
 * @returns The executable, or `undefined` when there is none.
 */
export function findBinary(settings: Settings, searchFrom?: string): string | undefined {
	return resolveBinary(
		{ configuredPath: settings.path, firstWorkspaceFolder: getFirstFolderPath(), searchFrom },
		createProcessEnvironment(),
	);
}

/**
 * Says sloptor could not be found, with a way to fix it. Shown once per
 * attempt; nothing retries on its own.
 *
 * @param settings - The settings the search used.
 * @returns Resolves once the message is dismissed or acted on.
 */
export async function showMissingBinaryAsync(settings: Settings): Promise<void> {
	const message =
		settings.path === undefined
			? "sloptor isn't installed here: there is no node_modules/.bin/rotor and no sloptor or rotor on PATH."
			: `sloptor.path is "${settings.path}", which is not an executable file.`;

	const choice = await window.showWarningMessage(message, OPEN_SETTINGS);
	if (choice === OPEN_SETTINGS) {
		await commands.executeCommand("workbench.action.openSettings", "sloptor.path");
	}
}

/**
 * Owns the one `sloptor lsp` client. Starts, stops and restarts run one at a
 * time, so a restart during a start waits for it instead of racing it.
 */
export class LanguageServer implements Disposable {
	private readonly getSettings: () => Settings;
	private readonly outputChannel: LogOutputChannel;

	private client: LanguageClient | undefined;
	private queue: Promise<void> = Promise.resolve();

	public constructor(outputChannel: LogOutputChannel, getSettings: () => Settings) {
		this.outputChannel = outputChannel;
		this.getSettings = getSettings;
	}

	public dispose(): void {
		this.stopAsync().catch(() => undefined);
	}

	/**
	 * Stops the server if it runs, then starts it with the current settings.
	 *
	 * @returns Resolves once the server is up, or failed to start.
	 */
	public async restartAsync(): Promise<void> {
		return this.enqueueAsync(async () => {
			await this.stopClientAsync();
			await this.startClientAsync();
		});
	}

	/**
	 * Sends a request to the server once any pending start has finished.
	 *
	 * @param method - A sloptor request, such as `sloptor/outputPath`.
	 * @param parameters - Its params object.
	 * @returns The result as it came off the wire.
	 * @rejects When the server is not running, or answers with an error.
	 */
	public async sendRequestAsync(method: string, parameters: JsonObject): Promise<JsonValue> {
		await this.queue;

		const { client } = this;
		if (client?.isRunning() !== true) {
			throw new Error("sloptor lsp is not running.");
		}

		return client.sendRequest<JsonValue>(method, parameters);
	}

	/**
	 * Starts the server unless it already runs.
	 *
	 * @returns Resolves once the server is up, or failed to start.
	 */
	public async startAsync(): Promise<void> {
		return this.enqueueAsync(async () => this.startClientAsync());
	}

	/**
	 * Stops the server if it runs.
	 *
	 * @returns Resolves once the server has exited.
	 */
	public async stopAsync(): Promise<void> {
		return this.enqueueAsync(async () => this.stopClientAsync());
	}

	private async enqueueAsync(task: () => Promise<void>): Promise<void> {
		this.queue = this.settleAsync(this.queue.then(task));
		return this.queue;
	}

	/** Logs a failed start or stop, so the next one in the queue runs. */
	private async settleAsync(task: Promise<void>): Promise<void> {
		try {
			await task;
		} catch (error) {
			this.outputChannel.error("sloptor lsp:", error);
		}
	}

	private async startClientAsync(): Promise<void> {
		if (this.client !== undefined) return;

		const settings = this.getSettings();
		const binary = findBinary(settings, getFirstFolderPath());
		if (binary === undefined) {
			// Not awaited: a restart should not wait on a dismissed toast.
			showMissingBinaryAsync(settings).catch(() => undefined);
			return;
		}

		const client = new LanguageClient(CLIENT_ID, "sloptor", async () => spawnServerAsync(binary), {
			documentSelector: DOCUMENT_SELECTOR,
			initializationOptions: { typeErrors: settings.typeErrors },
			outputChannel: this.outputChannel,
		});

		this.client = client;

		this.outputChannel.info(`Starting ${binary} lsp`);

		try {
			await client.start();
		} catch (error) {
			this.client = undefined;
			await client.dispose().catch(() => undefined);
			throw error;
		}
	}

	private async stopClientAsync(): Promise<void> {
		const { client } = this;
		if (client === undefined) return;

		this.client = undefined;
		await client.dispose();
	}
}
