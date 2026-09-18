const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const Module = require("node:module");

const workerRoot = process.cwd();
const sessionPath = path.join(workerRoot, "tools", "sidecar", "lib", "session.js");
const indexPath = path.join(workerRoot, "tools", "sidecar", "index.js");
const sidecarRequire = Module.createRequire(path.join(workerRoot, "tools", "sidecar", "package.json"));
const ts = sidecarRequire("typescript");
const mode = process.env.SESSION_RETENTION_SOAK_MODE ?? "candidate";
const baselineCommit = "5ce06e9e07914e56cb7f727a403b3994d6bf56f8";

function loadServer() {
	if (mode === "candidate") {
		return {
			SidecarServer: require(indexPath).SidecarServer,
			source: fs.readFileSync(sessionPath),
		};
	}
	if (mode !== "baseline") {
		throw new Error(`unknown SESSION_RETENTION_SOAK_MODE: ${mode}`);
	}
	const baselineModule = new Module(sessionPath, module);
	baselineModule.filename = sessionPath;
	baselineModule.paths = Module._nodeModulePaths(path.dirname(sessionPath));
	const source = execFileSync("git", ["show", `${baselineCommit}:tools/sidecar/lib/session.js`], {
		cwd: workerRoot,
		encoding: "buffer",
	});
	baselineModule._compile(source.toString("utf8"), sessionPath);
	return { SidecarServer: baselineModule.exports.SidecarServer, source };
}

function metadata(session) {
	return {
		actualPaths: session.actualPaths.size,
		pathAliases: session.pathAliases.size,
		versions: session.versions.size,
		deleted: session.deleted.size,
		baseRoots: session.baseRoots.size,
		rootLimit: session.rootLimit?.size ?? 0,
	};
}

function memory() {
	const { rss, heapUsed } = process.memoryUsage();
	return { rss, heapUsed };
}

function captureConfigSnapshot(configPath) {
	const files = {};
	const host = Object.create(ts.sys);
	host.readFile = (fileName) => {
		const text = ts.sys.readFile(fileName);
		if (text !== undefined) {
			files[fileName] = text;
		}
		return text;
	};
	assert.ok(ts.getParsedCommandLineOfConfigFile(configPath, {}, host));
	return files;
}

const { SidecarServer, source } = loadServer();
const originalCwd = process.cwd();
const root = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-session-retention-soak-"));
try {
	const sourceDir = path.join(root, "src");
	fs.mkdirSync(sourceDir);
	const main = path.join(sourceDir, "main.ts");
	const configPath = path.join(root, "tsconfig.json");
	fs.writeFileSync(main, "export const main = 1;\n");
	fs.writeFileSync(
		configPath,
		JSON.stringify({
			compilerOptions: { module: "CommonJS", moduleResolution: "Node", noLib: true, target: "ESNext" },
			files: ["src/main.ts"],
		}),
	);
	const server = new SidecarServer(ts);
	const transform = (fileNames, compileFileNames, changedFiles) => {
		const request = {
			protocol: mode === "baseline" ? 2 : 3,
			operation: "transform",
			projectDir: root,
			tsConfigPath: configPath,
			fileNames,
			compileFileNames,
			rootFileNames: fileNames,
			changedFiles,
			fileContentIdentities: Object.fromEntries(
				fileNames.map((fileName) => [
					fileName,
					crypto.createHash("sha256").update(fs.readFileSync(fileName)).digest("hex"),
				]),
			),
		};
		if (mode === "candidate") {
			request.configSnapshot = captureConfigSnapshot(configPath);
		}
		const response = server.handleRequest(request);
		assert.deepEqual(response.diagnostics, []);
	};

	transform([main], [main], []);
	const samples = [{ cycle: 0, metadata: metadata(server.session), memory: memory() }];
	for (let index = 0; index < 300; index += 1) {
		const transient = path.join(sourceDir, `transient-${index}.ts`);
		const text = `export const value${index} = ${index};\n`;
		fs.writeFileSync(transient, text);
		transform([main, transient], [transient], [{ fileName: transient, text }]);
		fs.rmSync(transient);
		transform([main], [main], [{ fileName: transient, deleted: true }]);
		if ((index + 1) % 50 === 0) {
			samples.push({ cycle: index + 1, metadata: metadata(server.session), memory: memory() });
		}
	}
	process.stdout.write(
		`${JSON.stringify({
			mode,
			cycles: 300,
			baselineCommit: mode === "baseline" ? baselineCommit : undefined,
			runtime: { node: process.version, platform: process.platform, arch: process.arch },
			sessionSourceSha256: crypto.createHash("sha256").update(source).digest("hex"),
			samples,
		})}\n`,
	);
	server.close();
} finally {
	process.chdir(originalCwd);
	fs.rmSync(root, { recursive: true, force: true });
}
