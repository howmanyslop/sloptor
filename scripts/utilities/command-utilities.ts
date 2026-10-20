import { argv, exit } from "node:process";
import { Command, ValidationError } from "@cliffy/command";
import { consola } from "consola";

import { ShowHelpError } from "$errors/show-help-error";
import { getScriptName } from "$functions/get-script-name";

import type { ConsolaInstance } from "consola";

export interface ExtraOptions {
	readonly showHelpWithoutArguments?: boolean | undefined;
}

function createBaseCommand(name: string, version: string, description: string, options?: ExtraOptions): Command {
	const baseCommand = new Command()
		.error((error, command) => {
			if (error instanceof ShowHelpError) {
				command.showHelp();
				exit(0);
			}

			if (error instanceof ValidationError) {
				command.showHelp();
				console.error(error.message);
				exit(error.exitCode);
			}

			console.error(error);
			exit(1);
		})
		.name(name)
		.version(version)
		.description(description)
		.help({
			colors: true,
			hints: true,
			types: true,
		});

	if (options?.showHelpWithoutArguments === true) {
		baseCommand.action(() => {
			throw new ShowHelpError("No command specified.");
		});
	}

	return baseCommand;
}

interface ScriptCommand {
	readonly command: Command;
	readonly log: ConsolaInstance;
}

/**
 * Creates the base command for the running script, named and tagged after the
 * script file.
 *
 * @param version - Shown by `--version`.
 * @param description - Shown at the top of `--help`.
 * @returns The command and a logger tagged with the script name.
 */
export function createScriptCommand(version: string, description: string): ScriptCommand {
	const name = getScriptName(true);
	return { command: createBaseCommand(name, version, description), log: consola.withTag(name) };
}

export async function parseScriptArgumentsAsync(command: Command): Promise<void> {
	await command.parse(argv.slice(2));
}
