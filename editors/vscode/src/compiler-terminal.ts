import { EventEmitter, window } from "vscode";

import { toTerminalText } from "./terminal-text";

import type { Disposable, Terminal } from "vscode";

/**
 * A pseudoterminal that keeps what is written before VS Code opens it, which
 * VS Code would otherwise drop.
 */
export class CompilerTerminal implements Disposable {
	private readonly name: string;
	private readonly onClose: () => void;
	private readonly writeEmitter = new EventEmitter<string>();

	private isOpen = false;
	private pending = new Array<string>();
	private terminal: Terminal | undefined;

	/**
	 * Creates nothing yet: the terminal appears on the first `show`.
	 *
	 * @param name - The terminal's tab title.
	 * @param onClose - Runs when the user closes the terminal.
	 */
	public constructor(name: string, onClose: () => void) {
		this.name = name;
		this.onClose = onClose;
	}

	public dispose(): void {
		this.terminal?.dispose();
		this.terminal = undefined;
		this.writeEmitter.dispose();
	}

	/** Reveals the terminal without taking focus, creating it if needed. */
	public show(): void {
		this.terminal ??= window.createTerminal({
			name: this.name,
			isTransient: true,
			pty: {
				close: (): void => {
					this.terminal = undefined;
					this.onClose();
				},
				onDidWrite: this.writeEmitter.event,
				open: (): void => {
					this.isOpen = true;

					for (const text of this.pending) {
						this.writeEmitter.fire(text);
					}

					this.pending = [];
				},
			},
		});
		this.terminal.show(true);
	}

	/**
	 * Writes process output, fixing its line endings for a raw terminal.
	 *
	 * @param text - The output.
	 */
	public write(text: string): void {
		const converted = toTerminalText(text);
		if (this.isOpen) {
			this.writeEmitter.fire(converted);
			return;
		}

		this.pending.push(converted);
	}

	/**
	 * Writes one line of the extension's own.
	 *
	 * @param text - The line, without a line ending.
	 */
	public writeLine(text: string): void {
		this.write(`${text}\n`);
	}
}
