import { kill, platform } from "node:process";
import { x } from "tinyexec";
import { commands, StatusBarAlignment, window, workspace } from "vscode";

import { CompilerTerminal } from "./compiler-terminal";
import { findBinary, showMissingBinaryAsync } from "./language-client";
import { describeExit } from "./terminal-text";

import type { ChildProcess } from "node:child_process";
import type { Disposable, StatusBarItem, WorkspaceFolder } from "vscode";

import type { Settings } from "./settings";

const RUNNING_CONTEXT = "sloptor.compilerRunning";
const TOGGLE_COMMAND = "sloptor.toggleCompiler";
const SHOW_OUTPUT = "Show Output";
const KILL_GRACE_MILLISECONDS = 3000;
const TYPESCRIPT_LANGUAGES: ReadonlySet<string> = new Set(["typescript", "typescriptreact"]);

interface Compilation {
	child: ChildProcess | undefined;
	readonly folder: WorkspaceFolder;
	killTimer: NodeJS.Timeout | undefined;
	stopping: boolean;
	terminal: CompilerTerminal | undefined;
}

function killTree(pid: number, signal: NodeJS.Signals): void {
	if (platform === "win32") {
		// tinyexec swallows a failed spawn, and then there is nothing to kill.
		x("taskkill", ["/pid", String(pid), "/T", "/F"]);
		return;
	}

	try {
		// The compiler leads its own group, which takes the npm shim's child too.
		kill(-pid, signal);
	} catch {
		// Already gone.
	}
}

function stopCompilation(compilation: Compilation): void {
	const pid = compilation.child?.pid;
	if (pid === undefined || compilation.stopping) return;

	compilation.stopping = true;
	compilation.terminal?.writeLine("Stopping sloptor…");
	killTree(pid, "SIGTERM");

	if (platform === "win32") return;

	compilation.killTimer = setTimeout(() => {
		killTree(pid, "SIGKILL");
	}, KILL_GRACE_MILLISECONDS);
	compilation.killTimer.unref();
}

function isLocalFolder(folder?: WorkspaceFolder): folder is WorkspaceFolder {
	return folder?.uri.scheme === "file";
}

function getTerminal(compilation: Compilation): CompilerTerminal {
	if (compilation.terminal !== undefined) return compilation.terminal;

	const terminal = new CompilerTerminal(`sloptor (${compilation.folder.name})`, () => {
		// Closing the terminal is how people stop what they can't see.
		if (compilation.terminal === terminal) {
			compilation.terminal = undefined;
		}

		terminal.dispose();
		stopCompilation(compilation);
	});

	compilation.terminal = terminal;
	return terminal;
}

/**
 * The folder the compiler commands act on: the active file's, or the only one.
 *
 * @returns The folder, or `undefined` when there is no local one to pick.
 */
function getActiveFolder(): undefined | WorkspaceFolder {
	const editor = window.activeTextEditor;
	if (editor !== undefined) {
		const folder = workspace.getWorkspaceFolder(editor.document.uri);
		return isLocalFolder(folder) ? folder : undefined;
	}

	const folders = workspace.workspaceFolders;
	const only = folders?.length === 1 ? folders[0] : undefined;
	return isLocalFolder(only) ? only : undefined;
}

/**
 * Runs `sloptor build --watch` (or whatever `sloptor.compiler.arguments`
 * says) per workspace folder, each streaming into its own terminal, with a
 * status bar button for the active folder.
 */
export class Compiler implements Disposable {
	private readonly compilations = new Map<string, Compilation>();
	private readonly getSettings: () => Settings;

	private isRunningContext: boolean | undefined;
	private statusBarItem: StatusBarItem | undefined;

	/**
	 * Starts nothing: the compiler runs only when a command asks.
	 *
	 * @param getSettings - Returns the current settings.
	 */
	public constructor(getSettings: () => Settings) {
		this.getSettings = getSettings;
	}

	public dispose(): void {
		for (const compilation of this.compilations.values()) {
			const pid = compilation.child?.pid;
			if (pid !== undefined) {
				killTree(pid, "SIGTERM");
			}

			clearTimeout(compilation.killTimer);
			compilation.terminal?.dispose();
		}

		this.compilations.clear();
		this.statusBarItem?.dispose();
		this.statusBarItem = undefined;
	}

	/**
	 * Updates the status bar button and the `sloptor.compilerRunning` context
	 * key for the active editor. Cheap enough to run on every editor switch.
	 */
	public refresh(): void {
		const folder = getActiveFolder();
		const isRunning = folder !== undefined && this.compilations.get(folder.uri.toString())?.child !== undefined;

		if (this.isRunningContext !== isRunning) {
			this.isRunningContext = isRunning;
			commands.executeCommand("setContext", RUNNING_CONTEXT, isRunning);
		}

		const languageId = window.activeTextEditor?.document.languageId;
		const isTypeScript = languageId !== undefined && TYPESCRIPT_LANGUAGES.has(languageId);
		if (folder === undefined || !isTypeScript || !this.getSettings().statusBarEnabled) {
			this.statusBarItem?.hide();
			return;
		}

		const item = this.getStatusBarItem();
		item.text = isRunning ? "$(debug-stop) sloptor" : "$(debug-start) sloptor";
		item.tooltip = isRunning
			? `Stop the sloptor compiler in ${folder.name}`
			: `Start the sloptor compiler in ${folder.name}`;
		item.show();
	}

	/**
	 * Starts the compiler in the active folder, or shows its terminal when it
	 * already runs.
	 *
	 * @returns Resolves once the process is spawned or the user has been told
	 *   why not.
	 */
	public async startAsync(): Promise<void> {
		const folder = getActiveFolder();
		if (folder === undefined) {
			await window.showErrorMessage("Start Compiler needs a file in a local workspace folder.");
			return;
		}

		const compilation = this.getCompilation(folder);
		const terminal = getTerminal(compilation);
		if (compilation.child !== undefined) {
			terminal.show();
			return;
		}

		const settings = this.getSettings();
		const binary = findBinary(settings, folder.uri.fsPath);
		if (binary === undefined) {
			await showMissingBinaryAsync(settings);
			return;
		}

		this.spawnCompiler(compilation, terminal, binary, settings.compilerArguments);
	}

	/**
	 * Stops the compiler in the active folder.
	 *
	 * @returns Resolves once the process is signalled or the user has been
	 *   told it was not running.
	 */
	public async stopAsync(): Promise<void> {
		const folder = getActiveFolder();
		const compilation = folder === undefined ? undefined : this.compilations.get(folder.uri.toString());
		if (compilation?.child === undefined) {
			await window.showInformationMessage("The sloptor compiler isn't running here.");
			return;
		}

		stopCompilation(compilation);
	}

	/**
	 * Stops the compiler in the active folder if it runs, and starts it if not.
	 *
	 * @returns Resolves once the compiler is started or stopping.
	 */
	public async toggleAsync(): Promise<void> {
		const folder = getActiveFolder();
		if (folder !== undefined && this.compilations.get(folder.uri.toString())?.child !== undefined) {
			await this.stopAsync();
			return;
		}

		await this.startAsync();
	}

	private async finishAsync(
		compilation: Compilation,
		child: ChildProcess,
		code: null | number,
		signal: NodeJS.Signals | null,
	): Promise<void> {
		if (compilation.child !== child) return;

		compilation.child = undefined;
		clearTimeout(compilation.killTimer);
		compilation.killTimer = undefined;

		const wasStopping = compilation.stopping;
		compilation.stopping = false;

		const exit = describeExit(code, signal);
		compilation.terminal?.writeLine(exit.message);
		this.refresh();

		if (wasStopping || !exit.failed) return;

		const choice = await window.showErrorMessage(exit.message, SHOW_OUTPUT);
		if (choice === SHOW_OUTPUT) {
			getTerminal(compilation).show();
		}
	}

	private getCompilation(folder: WorkspaceFolder): Compilation {
		const key = folder.uri.toString();
		const existing = this.compilations.get(key);
		if (existing !== undefined) return existing;

		const compilation: Compilation = {
			child: undefined,
			folder,
			killTimer: undefined,
			stopping: false,
			terminal: undefined,
		};

		this.compilations.set(key, compilation);
		return compilation;
	}

	private getStatusBarItem(): StatusBarItem {
		if (this.statusBarItem !== undefined) return this.statusBarItem;

		const item = window.createStatusBarItem("sloptor.compiler", StatusBarAlignment.Left, 0);
		item.name = "sloptor compiler";
		item.command = TOGGLE_COMMAND;
		this.statusBarItem = item;
		return item;
	}

	private spawnCompiler(
		compilation: Compilation,
		terminal: CompilerTerminal,
		binary: string,
		compilerArguments: ReadonlyArray<string>,
	): void {
		terminal.show();
		terminal.writeLine(`> ${[binary, ...compilerArguments].join(" ")}`);

		const child = x(binary, compilerArguments, {
			nodeOptions: {
				cwd: compilation.folder.uri.fsPath,
				env: { FORCE_COLOR: "1" },
				stdio: ["ignore", "pipe", "pipe"],
			},
			// Its own process group, so stopping reaches every process under it.
			// Windows has `taskkill /T`, and detaching there opens a console.
			persist: platform !== "win32",
		}).process;

		if (child === undefined) return;

		compilation.child = child;
		compilation.stopping = false;

		// Not tinyexec's line iterator: it holds back partial lines, and watch
		// mode redraws in place.
		child.stdout?.setEncoding("utf8");
		child.stderr?.setEncoding("utf8");
		child.stdout?.on("data", (chunk: string) => compilation.terminal?.write(chunk));
		child.stderr?.on("data", (chunk: string) => compilation.terminal?.write(chunk));
		child.on("error", (error) => {
			const message = `Couldn't run ${binary}: ${error.message}`;
			compilation.terminal?.writeLine(message);
			// The close that follows a failed spawn is not a second failure.
			compilation.stopping = true;
			window.showErrorMessage(message);
		});
		child.on("close", (code, signal) => {
			// Only the error toast can reject, and there is nobody left to tell.
			this.finishAsync(compilation, child, code, signal).catch(() => undefined);
		});

		this.refresh();
	}
}
