import nodePath from "node:path";
import { argv } from "node:process";
import { regex } from "arktype";

const SUFFIXES_REGEXP = regex("-command$", "gu");

export function getScriptName(trimEnd = true, scriptPath = argv[1] ?? import.meta.filename): string {
	let scriptName = nodePath.basename(scriptPath, nodePath.extname(scriptPath));
	if (scriptName === "index") scriptName = nodePath.basename(nodePath.dirname(scriptPath));
	return trimEnd ? scriptName.replaceAll(SUFFIXES_REGEXP, "") : scriptName;
}
