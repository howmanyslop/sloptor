export interface ExitDescription {
	readonly failed: boolean;
	readonly message: string;
}

const LINE_FEED = /\r?\n/gu;

/**
 * A pseudoterminal is a raw terminal: a bare `\n` moves down without
 * returning to the first column.
 *
 * @param text - Process output.
 * @returns The text with every line ending as `\r\n`.
 */
export function toTerminalText(text: string): string {
	return text.replaceAll(LINE_FEED, "\r\n");
}

/**
 * Describes how the compiler ended, for the terminal and for the failure
 * notification.
 *
 * @param code - The exit code, or `null` when a signal ended it.
 * @param signal - The signal that ended it, if any.
 * @returns A sentence, and whether it counts as a failure.
 */
export function describeExit(code: null | number, signal: NodeJS.Signals | null): ExitDescription {
	if (code !== null) {
		return { failed: code !== 0, message: `sloptor exited with code ${code}.` };
	}

	if (signal !== null) {
		return { failed: true, message: `sloptor was killed by ${signal}.` };
	}

	return { failed: true, message: "sloptor exited." };
}
