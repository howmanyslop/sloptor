const path = require("node:path");

const root = process.argv[2];
const configPath = path.join(root, "tsconfig.json");
process.chdir(root);

const ts = require(require.resolve("typescript", { paths: [root] }));
const transformer = require(require.resolve("rbxts-transformer-flamework", { paths: [root] }));
const configFile = ts.readConfigFile(configPath, ts.sys.readFile);
const parsed = ts.parseJsonConfigFileContent(configFile.config, ts.sys, root, undefined, configPath);
const program = ts.createProgram(parsed.fileNames, parsed.options);
const source = program.getSourceFile(path.join(root, "invalid", "case.ts"));

function serializeDiagnostic(diagnostic) {
	return {
		category: ts.DiagnosticCategory[diagnostic.category].toLowerCase(),
		code: String(diagnostic.code),
		file: diagnostic.file?.fileName,
		start: diagnostic.start,
		length: diagnostic.length,
		message: ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
		related: (diagnostic.relatedInformation ?? []).map(serializeDiagnostic),
	};
}

let diagnostics;
try {
	const factory = transformer.default ?? transformer;
	const result = ts.transform(source, [factory(program, {
		noSemanticDiagnostics: true,
		salt: "task7-controlled-salt",
		hashPrefix: "task7",
		idGenerationMode: "full",
	})]);
	diagnostics = result.diagnostics.map(serializeDiagnostic);
	result.dispose();
} catch (error) {
	diagnostics = [{
		category: "error",
		code: "upstream-internal",
		message: error instanceof Error ? error.stack ?? error.message : String(error),
		related: [],
	}];
}

process.stdout.write(`TASK7_JSON:${JSON.stringify(diagnostics)}\n`);
