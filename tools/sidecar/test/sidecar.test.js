const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawn, spawnSync } = require("node:child_process");
const test = require("node:test");
const ts = require("typescript");

const sidecar = require("../index.js");

const projectDir = path.resolve(__dirname, "..", "testdata", "project");
const tsConfigPath = path.join(projectDir, "tsconfig.json");
const sourcePath = path.join(projectDir, "src", "example.ts");
const mainPath = path.resolve(__dirname, "..", "main.js");
const checkerReferenceProjectDir = path.resolve(__dirname, "..", "testdata", "checker-project-reference", "app");
const checkerReferenceConfigPath = path.join(checkerReferenceProjectDir, "tsconfig.json");
const checkerReferenceSourcePath = path.join(checkerReferenceProjectDir, "src", "main.ts");

// TypeScript reports file names with forward slashes on every platform, so
// comparisons against host paths must be slash-normalized to hold on Windows.
function toSlash(value) {
  try {
    return fs.realpathSync.native?.(value).replace(/\\/g, "/") ?? fs.realpathSync(value).replace(/\\/g, "/");
  } catch {
    return value.replace(/\\/g, "/");
  }
}

function contentIdentity(text) {
  return crypto.createHash("sha256").update(text).digest("hex");
}

function transformRequest(request) {
  if (request.operation !== "transform" || request.fileContentIdentities !== undefined) {
    return request;
  }
  const changed = new Map((request.changedFiles ?? [])
    .filter((file) => typeof file.text === "string")
    .map((file) => [toSlash(file.fileName), file.text]));
  const fileContentIdentities = {};
  for (const fileName of request.fileNames ?? []) {
    const text = changed.get(toSlash(fileName)) ?? ts.sys.readFile(fileName);
    assert.notEqual(text, undefined, `expected authoritative text for ${fileName}`);
    fileContentIdentities[fileName] = contentIdentity(text);
  }
  return { ...request, fileContentIdentities };
}

function createProtocolServer(typeScript = ts) {
  const server = new sidecar.SidecarServer(typeScript);
  const handleRequest = server.handleRequest.bind(server);
  server.handleRequest = (request) => handleRequest(transformRequest(request));
  return server;
}

function resolveOptions(configPath) {
  const parsed = ts.getParsedCommandLineOfConfigFile(configPath, {}, ts.sys);
  if (!parsed) {
    throw new Error("expected parsed tsconfig");
  }

  return parsed;
}

function createProgram() {
  const parsed = resolveOptions(tsConfigPath);

  return ts.createProgram({
    rootNames: parsed.fileNames,
    options: parsed.options,
    projectReferences: parsed.projectReferences,
  });
}

function checkerReferenceOracle() {
  const parsed = resolveOptions(checkerReferenceConfigPath);
  const builder = ts.createEmitAndSemanticDiagnosticsBuilderProgram(
    parsed.fileNames,
    parsed.options,
    ts.createIncrementalCompilerHost(parsed.options),
  );
  const program = builder.getProgram();
  const sourceFile = program.getSourceFile(checkerReferenceSourcePath);
  assert.ok(sourceFile, "expected checker-reference source file");
  const probe = sourceFile.statements
    .filter(ts.isVariableStatement)
    .flatMap((statement) => statement.declarationList.declarations)
    .find((declaration) => ts.isIdentifier(declaration.name) && declaration.name.text === "probe");
  assert.ok(probe?.initializer, "expected checker-reference probe initializer");
  return program.getTypeChecker().typeToString(program.getTypeChecker().getTypeAtLocation(probe.initializer));
}

function describePluginConfigs(configs) {
  return configs.map((config) => ({
    type: config.type ?? "program",
    import: config.import ?? "default",
    prefix: config.prefix,
    after: Boolean(config.after),
    afterDeclarations: Boolean(config.afterDeclarations),
  }));
}

const PROJECT_PLUGIN_CONFIGS = [
  { type: "program", import: "default", prefix: "after", after: true, afterDeclarations: false },
  { type: "config", import: "configTransformer", prefix: "before", after: false, afterDeclarations: false },
  { type: "raw", import: "rawTransformer", prefix: "afterDeclarations", after: false, afterDeclarations: true },
];

function readProtocolLine(stream) {
  return new Promise((resolve, reject) => {
    let buffer = "";

    function cleanup() {
      stream.off("data", onData);
      stream.off("error", onError);
    }

    function onError(error) {
      cleanup();
      reject(error);
    }

    function onData(chunk) {
      buffer += chunk.toString();
      const newlineIndex = buffer.indexOf("\n");
      if (newlineIndex === -1) {
        return;
      }

      const line = buffer.slice(0, newlineIndex);
      cleanup();
      resolve(JSON.parse(line));
    }

    stream.on("data", onData);
    stream.on("error", onError);
  });
}

// The three tests below pin `extends` resolution to what `tsc --showConfig`
// reports: `plugins` is an array-valued compiler option, so a child that
// declares it REPLACES the parent's list rather than adding to it. rbxtsc
// concatenates the whole chain instead, which leaves a child no way to opt out
// of an inherited transform.
test("getPluginConfigs takes the child's plugins in place of the parent's", () => {
  const configs = sidecar.getPluginConfigs(resolveOptions(tsConfigPath).options);

  assert.deepEqual(describePluginConfigs(configs), PROJECT_PLUGIN_CONFIGS);
  assert.equal(
    configs.some((config) => config.prefix === "inherited"),
    false,
    "expected the parent's replaced plugin to be absent",
  );
});

test("getPluginConfigs honours a child that overrides plugins with an empty array", () => {
  const configs = sidecar.getPluginConfigs(resolveOptions(path.join(projectDir, "tsconfig.no-plugins.json")).options);

  assert.deepEqual(configs, []);
});

test("getPluginConfigs inherits the parent's plugins when the child declares none", () => {
  const configs = sidecar.getPluginConfigs(resolveOptions(path.join(projectDir, "tsconfig.inherit.json")).options);

  assert.deepEqual(describePluginConfigs(configs), PROJECT_PLUGIN_CONFIGS);
});

test("a project whose plugins are overridden to empty runs no transformers", () => {
  const configPath = path.join(projectDir, "tsconfig.no-plugins.json");
  const session = new sidecar.SidecarProjectSession(ts, projectDir, configPath);
  const response = session.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir,
    tsConfigPath: configPath,
    fileNames: [sourcePath],
    compileFileNames: [sourcePath],
    changedFiles: [],
  });

  assert.deepEqual(response.diagnostics, []);
  assert.deepEqual(response.transformed, []);
});

// Rotor emits declarations natively (tsgo has no afterDeclarations hook), so
// the worker reports how many of these it built instead of running them; rotor
// turns a non-zero count into a one-shot per-project warning.
test("responses report the afterDeclarations transformer count", () => {
  const session = new sidecar.SidecarProjectSession(ts, projectDir, tsConfigPath);
  const response = session.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir,
    tsConfigPath,
    fileNames: [sourcePath],
    compileFileNames: [sourcePath],
    changedFiles: [],
  });

  assert.deepEqual(response.diagnostics, []);
  assert.equal(response.afterDeclarationsTransformers, 1);
});

test("a project with no plugins reports no afterDeclarations transformers", () => {
  const configPath = path.join(projectDir, "tsconfig.no-plugins.json");
  const session = new sidecar.SidecarProjectSession(ts, projectDir, configPath);
  const response = session.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir,
    tsConfigPath: configPath,
    fileNames: [sourcePath],
    compileFileNames: [sourcePath],
    changedFiles: [],
  });

  assert.equal(response.afterDeclarationsTransformers, 0);
});

// Catches a checker plugin observing a referenced project's already-emitted
// declarations instead of the source program rbxtsc gives it. The expected
// literal comes from the rbxtsc createProjectProgram builder construction,
// which uses parsed roots and options without parsed project references.
test("checker plugins use the referenced source contract from rbxtsc", () => {
  const expected = checkerReferenceOracle();
  assert.equal(expected, "1");
  const session = new sidecar.SidecarProjectSession(ts, checkerReferenceProjectDir, checkerReferenceConfigPath);
  const response = session.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir: checkerReferenceProjectDir,
    tsConfigPath: checkerReferenceConfigPath,
    fileNames: [checkerReferenceSourcePath],
    compileFileNames: [checkerReferenceSourcePath],
    changedFiles: [],
  });

  assert.deepEqual(response.diagnostics, []);
  assert.equal(response.transformed.length, 1);
  assert.match(response.transformed[0].text, new RegExp(`checkerProbe = "${expected}"`));
});

test("createTransformerList instantiates checker and compilerOptions factories", () => {
  const program = createProgram();
  const sourceFile = program.getSourceFile(sourcePath);
  assert.ok(sourceFile, "expected source file");

  const { transforms, diagnostics } = sidecar.createTransformerList(ts, program, [
    {
      transform: "./plugins/prefix-string-named.js",
      import: "compilerOptionsTransformer",
      type: "compilerOptions",
      prefix: "options",
      after: true,
    },
    {
      transform: "./plugins/prefix-string-named.js",
      import: "checkerTransformer",
      type: "checker",
      prefix: "checker",
    },
  ], projectDir);

  assert.deepEqual(diagnostics, []);

  const result = sidecar.transformSourceFiles(ts, program, [sourceFile], transforms);
  assert.deepEqual(
    result.diagnostics,
    [],
  );
  assert.match(result.transformed[0].text, /options:checker:start/);
});

test("createTransformerList reports a non-function export as an error", () => {
  const program = createProgram();
  const { diagnostics } = sidecar.createTransformerList(ts, program, [
    {
      transform: "./plugins/prefix-string-named.js",
      import: "missingTransformer",
    },
  ], projectDir);

  assert.equal(diagnostics.length, 1);
  assert.equal(diagnostics[0].category, "error");
  assert.equal(diagnostics[0].code, "transformer-not-found");
  assert.match(diagnostics[0].message, /Transformer `\.\/plugins\/prefix-string-named\.js` failed to load!/);
  assert.match(diagnostics[0].message, /factory not a function/);
  assert.match(diagnostics[0].message, /Suggestion: Did you forget to install the package, or to build it\?/);
});

test("transformSourceFiles omits source files whose transformers preserve identity", () => {
  const program = createProgram();
  const sourceFile = program.getSourceFile(sourcePath);
  assert.ok(sourceFile, "expected source file");

  const result = sidecar.transformSourceFiles(ts, program, [sourceFile], {
    before: [() => (file) => file],
    after: [],
    afterDeclarations: [],
  });

  assert.deepEqual(result.diagnostics, []);
  assert.deepEqual(result.transformed, []);
});

test("sidecar protocol requires an explicit operation", () => {
  const server = createProtocolServer(ts);
  const response = server.handleRequest({
    protocol: 2,
    projectDir,
    tsConfigPath,
    compileFileNames: [sourcePath],
    changedFiles: [],
  });

  assert.deepEqual(response.transformed, []);
  assert.equal(response.diagnostics.length, 1);
  assert.equal(response.diagnostics[0].code, "invalid-request");
  assert.match(response.diagnostics[0].message, /operation must equal/);
});

test("sidecar protocol requires content identities and accepts an empty project", (t) => {
  const server = new sidecar.SidecarServer(ts);
  const missing = server.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir,
    tsConfigPath,
    fileNames: [sourcePath],
    compileFileNames: [sourcePath],
    changedFiles: [],
  });
  const emptyProjectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-empty-"));
  t.after(() => fs.rmSync(emptyProjectDir, { recursive: true, force: true }));
  const emptyConfigPath = path.join(emptyProjectDir, "tsconfig.json");
  fs.writeFileSync(emptyConfigPath, JSON.stringify({ compilerOptions: { noLib: true }, files: [] }));
  const empty = server.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir: emptyProjectDir,
    tsConfigPath: emptyConfigPath,
    fileNames: [],
    compileFileNames: [],
    fileContentIdentities: {},
    changedFiles: [],
  });

  assert.equal(missing.diagnostics[0].code, "invalid-request");
  assert.match(missing.diagnostics[0].message, /fileContentIdentities/);
  assert.equal(empty.diagnostics.some((diagnostic) => diagnostic.code === "invalid-request"), false);
  server.close();
});

// Catches comment-driven compiler directives disappearing before later
// transformers can observe the reprinted source. The expected comment comes
// from the upstream intermediate-printer contract described by issue #39.
test("intermediate transforms preserve comments when final emit removes them", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-comments-"));
  const sourceDir = path.join(fixtureDir, "src");
  fs.mkdirSync(sourceDir, { recursive: true });
  const fileName = path.join(sourceDir, "main.ts");
  fs.writeFileSync(fileName, "// transformer-directive\nexport const phase = \"start\";\n");
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `module.exports = (program, config, helpers) => (context) => {
  const visit = (node) => helpers.ts.isStringLiteral(node)
    ? helpers.ts.factory.createStringLiteral("changed")
    : helpers.ts.visitEachChild(node, visit, context);
  return (sourceFile) => helpers.ts.visitNode(sourceFile, visit);
};\n`);
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: {
      module: "CommonJS", moduleResolution: "Node", noLib: true,
      removeComments: true, target: "ESNext", rootDir: "src", outDir: "out",
      plugins: [{ transform: "./plugin.js" }],
    },
    include: ["src"],
  }));

  const response = new sidecar.SidecarProjectSession(ts, fixtureDir, configPath).handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
    fileNames: [fileName],
    compileFileNames: [fileName],
    changedFiles: [],
  });

  assert.deepEqual(response.diagnostics, []);
  assert.match(response.transformed[0].text, /\/\/ transformer-directive/);
  assert.match(response.transformed[0].text, /changed/);
});

// Catches a transformer assigning source identities outside the project when
// an editor starts the sidecar through a symlinked project path. The expected
// `out` comes from the configured output directory relative to that project,
// which must be the same physical directory TypeScript used for its options.
test("main.js keeps plugin project arguments aligned with canonical paths", (t) => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-project-path-"));
  const aliasDir = path.join(path.dirname(fixtureDir), `${path.basename(fixtureDir)}-alias`);
  t.after(() => {
    fs.rmSync(aliasDir, { force: true });
    fs.rmSync(fixtureDir, { recursive: true, force: true });
  });
  try {
    fs.symlinkSync(fixtureDir, aliasDir, "dir");
  } catch (error) {
    t.skip(`cannot create project-path symlink: ${error.message}`);
    return;
  }
  const sourceDir = path.join(fixtureDir, "src");
  fs.mkdirSync(sourceDir, { recursive: true });
  fs.writeFileSync(path.join(sourceDir, "main.ts"), "export const value = 1;\n");
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `const path = require("node:path");
module.exports = (program, config, helpers) => (context) => (sourceFile) => {
  const flag = process.argv.findIndex((value) => value === "-p" || value === "--project");
  const projectDir = path.dirname(process.argv[flag + 1]);
  const relativeOut = path.relative(projectDir, program.getCompilerOptions().outDir);
  return helpers.ts.factory.updateSourceFile(sourceFile, [
    ...sourceFile.statements,
    helpers.ts.factory.createExpressionStatement(helpers.ts.factory.createStringLiteral(relativeOut)),
  ]);
};
`);
  fs.writeFileSync(path.join(fixtureDir, "tsconfig.json"), JSON.stringify({
    compilerOptions: {
      module: "CommonJS", moduleResolution: "Node", noLib: true,
      target: "ESNext", rootDir: "src", outDir: "out",
      plugins: [{ transform: "./plugin.js" }],
    },
    include: ["src"],
  }));

  const aliasConfigPath = path.join(aliasDir, "tsconfig.json");
  const result = spawnSync(process.execPath, [mainPath, "--project", aliasConfigPath], {
    input: `${JSON.stringify(transformRequest({
      protocol: 2,
      operation: "transform",
      projectDir: aliasDir,
      tsConfigPath: aliasConfigPath,
      compileFileNames: [path.join(aliasDir, "src", "main.ts")],
      fileNames: [path.join(aliasDir, "src", "main.ts")],
      changedFiles: [],
    }))}\n`,
    encoding: "utf8",
    cwd: aliasDir,
  });

  assert.equal(result.status, 0, result.stderr);
  const response = JSON.parse(result.stdout.trim());
  assert.deepEqual(response.diagnostics, []);
  assert.equal(response.transformed.length, 1);
  assert.match(response.transformed[0].text, /"out"/);
});

// Catches declaration validation executing plugin code even though declaration
// output never consumes transformed source. The absent marker is the plugin's
// own observable side effect, and the valid factory export is the config oracle.
test("validate loads plugin exports without invoking program or raw factories", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-validate-"));
  fs.writeFileSync(path.join(fixtureDir, "main.ts"), "export {};\n");
  const markerPath = path.join(fixtureDir, "factory-ran");
  const pluginPath = path.join(fixtureDir, "plugin.js");
  fs.writeFileSync(pluginPath, `const fs = require("node:fs");
module.exports = () => { fs.writeFileSync(${JSON.stringify(markerPath)}, "called"); throw new Error("must not run"); };\n`);
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(configPath, JSON.stringify({ compilerOptions: { plugins: [{ transform: "./plugin.js" }, { transform: "./plugin.js", type: "raw" }] }, include: ["main.ts"] }));

  const response = new sidecar.SidecarProjectSession(ts, fixtureDir, configPath).handleRequest({
    protocol: 2,
    operation: "validate",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
  });

  assert.deepEqual(response.diagnostics, []);
  assert.deepEqual(response.transformed, []);
  assert.equal(fs.existsSync(markerPath), false);
});

// Catches a plugin constructing compiler nodes with a separate TypeScript copy.
// The diagnostic code is the protocol contract; the two physical module paths
// are created independently so the test does not derive its expectation from implementation details.
test("validation rejects a plugin with a different TypeScript module instance", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-ts-instance-"));
  fs.writeFileSync(path.join(fixtureDir, "main.ts"), "export {};\n");
  const pluginDir = path.join(fixtureDir, "plugins");
  const pluginPath = path.join(pluginDir, "plugin.js");
  const pluginTypeScriptDir = path.join(pluginDir, "node_modules", "typescript");
  fs.mkdirSync(pluginTypeScriptDir, { recursive: true });
  fs.writeFileSync(pluginPath, "module.exports = () => () => (file) => file;\n");
  fs.writeFileSync(path.join(pluginTypeScriptDir, "package.json"), JSON.stringify({ name: "typescript", main: "index.js" }));
  fs.writeFileSync(path.join(pluginTypeScriptDir, "index.js"), "module.exports = {};\n");
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(configPath, JSON.stringify({ compilerOptions: { plugins: [{ transform: "./plugins/plugin.js" }] }, include: ["main.ts"] }));
  const sessionTypeScriptPath = fs.realpathSync(require.resolve("typescript"));

  const response = new sidecar.SidecarProjectSession(ts, fixtureDir, configPath, sessionTypeScriptPath).handleRequest({
    protocol: 2,
    operation: "validate",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
  });

  assert.equal(response.diagnostics.length, 1);
  assert.equal(response.diagnostics[0].code, "typescript-instance-mismatch");
});

// Catches plugin factory work being reported as worker overhead. The metric is
// an observable timing contract, and the delay is supplied by a real plugin.
test("plugin metrics include program and raw factory time", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-factory-timing-"));
  const sourceDir = path.join(fixtureDir, "src");
  fs.mkdirSync(sourceDir, { recursive: true });
  const fileName = path.join(sourceDir, "main.ts");
  fs.writeFileSync(fileName, "export const phase = 1;\n");
  fs.writeFileSync(path.join(fixtureDir, "slow-program.js"), `module.exports = () => {
  const until = Date.now() + 30; while (Date.now() < until) {}
  return () => (file) => file;
};\n`);
  fs.writeFileSync(path.join(fixtureDir, "slow-raw.js"), `module.exports = () => {
  const until = Date.now() + 30; while (Date.now() < until) {}
  return (file) => file;
};\n`);
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: {
      module: "CommonJS", moduleResolution: "Node", noLib: true,
      target: "ESNext", rootDir: "src", outDir: "out",
      plugins: [{ transform: "./slow-program.js" }, { transform: "./slow-raw.js", type: "raw" }],
    }, include: ["src"],
  }));

  const response = new sidecar.SidecarProjectSession(ts, fixtureDir, configPath).handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
    fileNames: [fileName],
    compileFileNames: [fileName],
    changedFiles: [],
  });

  assert.deepEqual(response.diagnostics, []);
  assert.deepEqual(response.metrics.plugins.map((plugin) => plugin.ms >= 20), [true, true]);
});

test("sidecar protocol responses include optional request metrics", () => {
  const server = createProtocolServer(ts);
  const response = server.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir,
    tsConfigPath,
    fileNames: [sourcePath],
    compileFileNames: [sourcePath],
    changedFiles: [],
  });

  assert.equal(typeof response.metrics.wallMs, "number");
  assert.ok(response.metrics.wallMs >= 0);
  assert.equal(typeof response.metrics.cpuUserUs, "number");
  assert.equal(typeof response.metrics.cpuSystemUs, "number");
  assert.equal(response.metrics.nodeVersion, process.version);
});

test("sidecar protocol metrics split wall time per transformer plugin", () => {
  const server = createProtocolServer(ts);
  const response = server.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir,
    tsConfigPath,
    fileNames: [sourcePath],
    compileFileNames: [sourcePath],
    changedFiles: [],
    plugins: [
      { transform: "./plugins/prefix-string.js", prefix: "first" },
      { transform: "./plugins/prefix-string.js", prefix: "second" },
    ],
  });

  assert.deepEqual(response.diagnostics, []);
  assert.deepEqual(
    response.metrics.plugins.map((plugin) => plugin.transform),
    ["./plugins/prefix-string.js", "./plugins/prefix-string.js"],
  );
  for (const plugin of response.metrics.plugins) {
    assert.equal(typeof plugin.ms, "number");
    assert.ok(plugin.ms >= 0);
  }
});
test("transformSourceFiles repairs orphaned parents between transformers", () => {
  const program = createProgram();
  const sourceFile = program.getSourceFile(sourcePath);
  assert.ok(sourceFile, "expected source file");

  const createSyntheticLiteral = (context) => {
    const visit = (node) => {
      if (ts.isStringLiteral(node) && node.text === "start") {
        return ts.factory.createStringLiteral("synthetic");
      }
      return ts.visitEachChild(node, visit, context);
    };
    return (file) => ts.visitNode(file, visit);
  };
  const requireSyntheticParent = (context) => {
    const visit = (node) => {
      if (ts.isStringLiteral(node) && node.text === "synthetic") {
        assert.ok(node.parent, "synthetic node must have a parent before the next transformer");
        assert.ok(ts.isVariableDeclaration(node.parent));
      }
      return ts.visitEachChild(node, visit, context);
    };
    return (file) => ts.visitNode(file, visit);
  };

  const result = sidecar.transformSourceFiles(ts, program, [sourceFile], {
    before: [createSyntheticLiteral, requireSyntheticParent],
    after: [],
    afterDeclarations: [],
  });

  assert.deepEqual(result.diagnostics, []);
  assert.match(result.transformed[0].text, /synthetic/);
});

test("main.js runs before then after, excludes afterDeclarations, and reuses overlay updates", async () => {
  const child = spawn(process.execPath, [mainPath], {
    cwd: path.resolve(__dirname, ".."),
    stdio: ["pipe", "pipe", "pipe"],
  });

  const stderr = [];
  child.stderr.on("data", (chunk) => {
    stderr.push(chunk.toString());
  });

  try {
    const firstResponsePromise = readProtocolLine(child.stdout);
    child.stdin.write(`${JSON.stringify(transformRequest({
      protocol: 2,
      operation: "transform",
      projectDir,
      tsConfigPath,
      fileNames: [sourcePath],
      compileFileNames: [sourcePath],
      changedFiles: [],
    }))}\n`);

    const firstResponse = await firstResponsePromise;
    assert.deepEqual(firstResponse.diagnostics, []);
    assert.equal(firstResponse.transformed.length, 1);
    assert.match(firstResponse.transformed[0].text, /after:before:start/);

    const secondResponsePromise = readProtocolLine(child.stdout);
    child.stdin.write(`${JSON.stringify(transformRequest({
      protocol: 2,
      operation: "transform",
      projectDir,
      tsConfigPath,
      fileNames: [sourcePath],
      compileFileNames: [sourcePath],
      changedFiles: [
        {
          fileName: sourcePath,
          text: 'export const phase = "memory";\n',
        },
      ],
    }))}\n`);

    const secondResponse = await secondResponsePromise;
    assert.deepEqual(secondResponse.diagnostics, []);
    assert.equal(secondResponse.transformed.length, 1);
    assert.match(secondResponse.transformed[0].text, /after:before:memory/);
  } finally {
    child.stdin.end();
    await new Promise((resolve) => child.once("exit", resolve));
  }

  assert.deepEqual(stderr, []);
});

test("main.js honors shouldTransformSourceFile and omits skipped source files", () => {
  const hookProjectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-hook-"));
  const sourceDir = path.join(hookProjectDir, "src");
  fs.mkdirSync(sourceDir, { recursive: true });

  fs.writeFileSync(
    path.join(hookProjectDir, "selective-plugin.js"),
    `const ts = require(${JSON.stringify(require.resolve("typescript"))});

function prefix(prefix) {
  return (context) => {
    const visit = (node) => {
      if (ts.isStringLiteral(node)) {
        return ts.factory.createStringLiteral(prefix + ":" + node.text);
      }
      return ts.visitEachChild(node, visit, context);
    };
    return (sourceFile) => ts.visitNode(sourceFile, visit);
  };
}

module.exports = function () {
  return {
    before: prefix("before"),
    after: prefix("after"),
    afterDeclarations: prefix("afterDeclarations"),
  };
};

module.exports.shouldTransformSourceFile = function (sourceFile, program, config) {
  if (!program.getTypeChecker() || config.transform !== "./selective-plugin.js") {
    throw new Error("missing source-file hook inputs");
  }
  return sourceFile.fileName.endsWith("selected.ts");
};
`,
  );
  fs.writeFileSync(
    path.join(hookProjectDir, "no-hooks-plugin.js"),
    `module.exports = function () { return {}; };
module.exports.shouldTransformSourceFile = true;
`,
  );
  fs.writeFileSync(
    path.join(hookProjectDir, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: {
        module: "CommonJS",
        moduleResolution: "Node",
        noLib: true,
        moduleDetection: "force",
        target: "ESNext",
        types: [],
        rootDir: "src",
        outDir: "out",
        plugins: [{ transform: "./selective-plugin.js" }, { transform: "./no-hooks-plugin.js" }],
      },
      include: ["src"],
    }),
  );
  const selectedPath = path.join(sourceDir, "selected.ts");
  const skippedPath = path.join(sourceDir, "skipped.ts");
  fs.writeFileSync(selectedPath, 'export const phase = "selected";\n');
  fs.writeFileSync(skippedPath, 'export const phase = "skipped";\n');

  const result = spawnSync(process.execPath, [mainPath], {
    input: `${JSON.stringify(transformRequest({
      protocol: 2,
      operation: "transform",
      projectDir: hookProjectDir,
      tsConfigPath: path.join(hookProjectDir, "tsconfig.json"),
      fileNames: [selectedPath, skippedPath],
      compileFileNames: [selectedPath, skippedPath],
      changedFiles: [],
    }))}\n`,
    encoding: "utf8",
    cwd: hookProjectDir,
  });

  assert.equal(result.status, 0, result.stderr);
  const response = JSON.parse(result.stdout.trim());
  assert.deepEqual(response.diagnostics, []);
  assert.equal(response.transformed.length, 1);
  assert.equal(toSlash(response.transformed[0].fileName), toSlash(selectedPath));
  assert.match(response.transformed[0].text, /after:before:selected/);
  assert.doesNotMatch(response.transformed[0].text, /afterDeclarations/);
});

test("main.js keeps plugin console.log off the protocol stream", () => {
  const noisyProjectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-stdout-"));
  fs.mkdirSync(path.join(noisyProjectDir, "src"), { recursive: true });
  fs.writeFileSync(
    path.join(noisyProjectDir, "noisy-plugin.js"),
    `module.exports = function (program, config) {
  console.log("plugin chatter on stdout");
  return (context) => (sourceFile) => sourceFile;
};\n`,
  );
  fs.writeFileSync(
    path.join(noisyProjectDir, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: {
        module: "CommonJS",
        moduleResolution: "Node",
        noLib: true,
        moduleDetection: "force",
        target: "ESNext",
        types: [],
        rootDir: "src",
        outDir: "out",
        plugins: [{ transform: "./noisy-plugin.js" }],
      },
      include: ["src"],
    }),
  );
  const noisyMainFile = path.join(noisyProjectDir, "src", "main.ts");
  fs.writeFileSync(noisyMainFile, 'export const phase = "start";\n');

  const request = JSON.stringify(transformRequest({
    protocol: 2,
    operation: "transform",
    tsConfigPath: path.join(noisyProjectDir, "tsconfig.json"),
    projectDir: noisyProjectDir,
    fileNames: [noisyMainFile],
    compileFileNames: [noisyMainFile],
    changedFiles: [],
  }));

  const result = spawnSync(process.execPath, [mainPath], {
    input: `${request}\n`,
    encoding: "utf8",
    cwd: noisyProjectDir,
  });

  assert.equal(result.status, 0, result.stderr);
  const lines = result.stdout.split("\n").filter((line) => line.trim().length > 0);
  assert.equal(lines.length, 1, `stdout must carry exactly one JSON response, got:\n${result.stdout}`);
  const response = JSON.parse(lines[0]);
  assert.deepEqual(response.diagnostics, []);
  assert.equal(result.stderr, "");
  assert.deepEqual(response.logs, ["plugin chatter on stdout"]);
});

test("main.js preserves complete plugin logs when a plugin exits immediately", (t) => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-exit-log-"));
  t.after(() => fs.rmSync(fixtureDir, { recursive: true, force: true }));
  const sourceDir = path.join(fixtureDir, "src");
  fs.mkdirSync(sourceDir, { recursive: true });
  const sourcePath = path.join(sourceDir, "main.ts");
  const diagnostic = `plugin diagnostic begins\n${"x".repeat(256 * 1024)}\nplugin diagnostic ends\n`;
  fs.writeFileSync(sourcePath, "export const value = 1;\n");
  fs.writeFileSync(
    path.join(fixtureDir, "exit-plugin.js"),
    `module.exports = function () {
  process.stderr.write(${JSON.stringify(diagnostic)});
  process.exit(17);
};
`,
  );
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(
    configPath,
    JSON.stringify({
      compilerOptions: {
        module: "CommonJS",
        moduleResolution: "Node",
        noLib: true,
        target: "ESNext",
        rootDir: "src",
        outDir: "out",
        plugins: [{ transform: "./exit-plugin.js" }],
      },
      include: ["src"],
    }),
  );

  const result = spawnSync(process.execPath, [mainPath], {
    input: `${JSON.stringify(transformRequest({
      protocol: 2,
      operation: "transform",
      projectDir: fixtureDir,
      tsConfigPath: configPath,
      fileNames: [sourcePath],
      compileFileNames: [sourcePath],
      changedFiles: [],
    }))}\n`,
    encoding: "utf8",
    cwd: fixtureDir,
  });

  assert.equal(result.status, 17);
  assert.equal(result.stdout, "");
  assert.equal(result.stderr.length, diagnostic.length);
  assert.ok(result.stderr.startsWith("plugin diagnostic begins\n"));
  assert.ok(result.stderr.endsWith("\nplugin diagnostic ends\n"));
});

test("resolveTypeScript prefers the project's typescript copy", () => {
  const stubProjectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-ts-"));
  const stubDir = path.join(stubProjectDir, "node_modules", "typescript");
  fs.mkdirSync(stubDir, { recursive: true });
  fs.writeFileSync(
    path.join(stubDir, "package.json"),
    JSON.stringify({ name: "typescript", version: "0.0.0-stub", main: "index.js" }),
  );
  fs.writeFileSync(path.join(stubDir, "index.js"), "module.exports = { __rotorStub: true };\n");

  const resolved = sidecar.resolveTypeScript(stubProjectDir);
  assert.equal(resolved.__rotorStub, true);
});

test("resolveTypeScript falls back to the sidecar's own typescript", () => {
  const emptyProjectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-nots-"));
  const resolved = sidecar.resolveTypeScript(emptyProjectDir);
  assert.equal(typeof resolved.transformNodes, "function");
});

// narrowRootsProject is a three-file project whose files do not import each
// other, so the root set alone decides what lands in the worker's program.
function narrowRootsProject() {
  const projectRoot = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-roots-"));
  const sourceDir = path.join(projectRoot, "src");
  fs.mkdirSync(sourceDir, { recursive: true });
  fs.writeFileSync(
    path.join(projectRoot, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: {
        module: "CommonJS",
        moduleResolution: "Node",
        noLib: true,
        strict: true,
        target: "ESNext",
        rootDir: "src",
        outDir: "out",
      },
      include: ["src"],
    }),
  );
  const files = {};
  for (const name of ["a", "b", "c"]) {
    files[name] = path.join(sourceDir, `${name}.ts`);
    fs.writeFileSync(files[name], `export const ${name} = 1;\n`);
  }
  files.globals = path.join(sourceDir, "globals.d.ts");
  fs.writeFileSync(files.globals, "declare const ambient: number;\n");
  const configPath = path.join(projectRoot, "tsconfig.json");
  return { projectRoot, configPath, files };
}

function programFileNames(session) {
  return session.service
    .getProgram()
    .getSourceFiles()
    .map((sourceFile) => toSlash(sourceFile.fileName))
    .sort();
}

function narrowRootsRequest(project, rootFileNames) {
  return {
    protocol: 2,
    operation: "transform",
    projectDir: project.projectRoot,
    tsConfigPath: project.configPath,
    compileFileNames: rootFileNames,
    fileNames: Object.values(project.files),
    rootFileNames,
    changedFiles: [],
  };
}

test("rootFileNames narrows the worker program to the files being compiled", () => {
  const project = narrowRootsProject();
  const session = new sidecar.SidecarProjectSession(ts, project.projectRoot, project.configPath);

  const response = session.handleRequest(narrowRootsRequest(project, [project.files.a, project.files.globals]));

  assert.deepEqual(response.diagnostics, []);
  assert.deepEqual(programFileNames(session), [toSlash(project.files.a), toSlash(project.files.globals)]);
});

test("the worker's root limit only ever widens within a session", () => {
  const project = narrowRootsProject();
  const session = new sidecar.SidecarProjectSession(ts, project.projectRoot, project.configPath);

  session.handleRequest(narrowRootsRequest(project, [project.files.a]));
  session.handleRequest(narrowRootsRequest(project, [project.files.b]));
  assert.deepEqual(programFileNames(session), [toSlash(project.files.a), toSlash(project.files.b)]);

  const full = narrowRootsRequest(project, [project.files.c]);
  delete full.rootFileNames;
  const response = session.handleRequest(full);
  assert.deepEqual(response.diagnostics, []);
  assert.deepEqual(programFileNames(session), [
    toSlash(project.files.a),
    toSlash(project.files.b),
    toSlash(project.files.c),
    toSlash(project.files.globals),
  ]);

  session.handleRequest(narrowRootsRequest(project, [project.files.a]));
  assert.equal(session.rootLimit, undefined);
});

test("rootFileNames must be an array of strings when present", () => {
  const project = narrowRootsProject();
  const server = createProtocolServer(ts);
  const response = server.handleRequest({ ...narrowRootsRequest(project, [project.files.a]), rootFileNames: [1] });

  assert.equal(response.diagnostics.length, 1);
  assert.match(response.diagnostics[0].message, /rootFileNames must be an array of strings/);
});

test("a deleted overlay removes a file before the next program is built", () => {
  const project = narrowRootsProject();
  const session = new sidecar.SidecarProjectSession(ts, project.projectRoot, project.configPath);

  session.handleRequest(narrowRootsRequest(project, [project.files.a, project.files.b]));
  assert.deepEqual(programFileNames(session), [toSlash(project.files.a), toSlash(project.files.b)]);

  const request = narrowRootsRequest(project, [project.files.a]);
  request.fileNames = request.fileNames.filter((fileName) => fileName !== project.files.b);
  request.changedFiles = [{ fileName: project.files.b, deleted: true }];
  const response = session.handleRequest(request);

  assert.deepEqual(response.diagnostics, []);
  assert.equal(session.readFile(project.files.b), undefined);
  assert.deepEqual(programFileNames(session), [toSlash(project.files.a)]);
});

test("a complete fileNames snapshot invalidates parsed config file discovery", () => {
  const project = narrowRootsProject();
  const session = new sidecar.SidecarProjectSession(ts, project.projectRoot, project.configPath);
  const full = narrowRootsRequest(project, [project.files.a]);
  delete full.rootFileNames;
  session.handleRequest(full);

  const addedFile = path.join(path.dirname(project.files.a), "added.ts");
  fs.writeFileSync(addedFile, "export const added = 1;\n");
  const next = narrowRootsRequest(project, [addedFile]);
  next.fileNames.push(addedFile);
  delete next.rootFileNames;
  const response = session.handleRequest(next);

  assert.deepEqual(response.diagnostics, []);
  assert.ok(programFileNames(session).includes(toSlash(addedFile)));
});

test("warm accepts no roots or overlays and builds the tsconfig program", () => {
  const project = narrowRootsProject();
  const server = createProtocolServer(ts);
  const response = server.handleRequest({
    protocol: 2,
    operation: "warm",
    projectDir: project.projectRoot,
    tsConfigPath: project.configPath,
  });

  assert.deepEqual(response.diagnostics, []);
  assert.deepEqual(programFileNames(server.session), [
    toSlash(project.files.a),
    toSlash(project.files.b),
    toSlash(project.files.c),
    toSlash(project.files.globals),
  ]);
  server.close();
});

test("transform retains maps until an explicit release", () => {
  const server = createProtocolServer(ts);
  const transform = server.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir,
    tsConfigPath,
    fileNames: [sourcePath],
    compileFileNames: [sourcePath],
    changedFiles: [],
  });

  assert.deepEqual(transform.diagnostics, []);
  assert.equal(typeof transform.resultHandle, "string");
  assert.equal("traceMap" in transform.transformed[0], false);

  const maps = server.handleRequest({
    protocol: 2,
    operation: "maps",
    projectDir,
    tsConfigPath,
    resultHandle: transform.resultHandle,
    fileNames: [sourcePath],
  });
  assert.deepEqual(maps.diagnostics, []);
  assert.equal(maps.traceMaps.length, 1);
  assert.doesNotThrow(() => JSON.parse(maps.traceMaps[0].traceMap));

  const release = server.handleRequest({
    protocol: 2,
    operation: "release",
    projectDir,
    tsConfigPath,
    resultHandle: transform.resultHandle,
    outcome: "success",
  });
  assert.deepEqual(release.diagnostics, []);

  const afterRelease = server.handleRequest({
    protocol: 2,
    operation: "maps",
    projectDir,
    tsConfigPath,
    resultHandle: transform.resultHandle,
  });
  assert.equal(afterRelease.diagnostics[0].code, "invalid-result-handle");
  server.close();
});

test("a cached transformer keeps its state for the same Program and exact config", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-stateful-"));
  const sourceFile = path.join(fixtureDir, "main.ts");
  fs.writeFileSync(sourceFile, 'export const phase = "input";\n');
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `module.exports = (program, config, helpers) => {
  let sequence = 0;
  return (context) => {
    const visit = (node) => helpers.ts.isStringLiteral(node)
      ? helpers.ts.factory.createStringLiteral(config.prefix + ":" + ++sequence)
      : helpers.ts.visitEachChild(node, visit, context);
    return (source) => helpers.ts.visitNode(source, visit);
  };
};\n`);
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(configPath, JSON.stringify({ compilerOptions: { noLib: true }, files: ["main.ts"] }));
  const server = createProtocolServer(ts);
  const request = {
    protocol: 2,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
    fileNames: [sourceFile],
    compileFileNames: [sourceFile],
    changedFiles: [],
    plugins: [{ transform: "./plugin.js", prefix: "stable" }],
  };

  const first = server.handleRequest(request);
  const second = server.handleRequest(request);
  const changedConfig = server.handleRequest({
    ...request,
    plugins: [{ transform: "./plugin.js", prefix: "changed" }],
  });

  assert.match(first.transformed[0].text, /"stable:1"/);
  assert.match(second.transformed[0].text, /"stable:2"/);
  assert.match(changedConfig.transformed[0].text, /"changed:1"/);
  server.close();
});

test("editing an extended config changes the effective transformer config", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-config-chain-"));
  const sourceFile = path.join(fixtureDir, "main.ts");
  const baseConfig = path.join(fixtureDir, "base.json");
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(sourceFile, 'export const phase = "input";\n');
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `module.exports = (program, config, helpers) => (context) => {
  const visit = (node) => helpers.ts.isStringLiteral(node)
    ? helpers.ts.factory.createStringLiteral(config.prefix)
    : helpers.ts.visitEachChild(node, visit, context);
  return (source) => helpers.ts.visitNode(source, visit);
};\n`);
  const writeBase = (prefix) => fs.writeFileSync(baseConfig, JSON.stringify({
    compilerOptions: { noLib: true, plugins: [{ transform: "./plugin.js", prefix }] },
  }));
  writeBase("from-base-one");
  fs.writeFileSync(configPath, JSON.stringify({ extends: "./base.json", files: ["main.ts"] }));
  const server = createProtocolServer(ts);
  const request = {
    protocol: 2,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
    fileNames: [sourceFile],
    compileFileNames: [sourceFile],
    changedFiles: [],
  };

  const first = server.handleRequest(request);
  writeBase("from-base-two");
  const second = server.handleRequest(request);

  assert.match(first.transformed[0].text, /"from-base-one"/);
  assert.match(second.transformed[0].text, /"from-base-two"/);
  server.close();
});

test("editing a loaded plugin dependency is visible on the next transform", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-plugin-freshness-"));
  const sourceFile = path.join(fixtureDir, "main.ts");
  const valueModule = path.join(fixtureDir, "value.js");
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(sourceFile, 'export const phase = "input";\n');
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `module.exports = (program, config, helpers) => {
  return (context) => {
    return (source) => {
      const value = require("./value.js");
      const visit = (node) => helpers.ts.isStringLiteral(node)
        ? helpers.ts.factory.createStringLiteral(value)
        : helpers.ts.visitEachChild(node, visit, context);
      return helpers.ts.visitNode(source, visit);
    };
  };
};\n`);
  fs.writeFileSync(valueModule, 'module.exports = "dependency-one";\n');
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: { noLib: true, plugins: [{ transform: "./plugin.js" }] },
    files: ["main.ts"],
  }));
  const server = createProtocolServer(sidecar.resolveTypeScript);
  const request = {
    protocol: 2,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
    fileNames: [sourceFile],
    compileFileNames: [sourceFile],
    changedFiles: [],
  };

  const first = server.handleRequest(request);
  fs.writeFileSync(valueModule, 'module.exports = "dependency-two";\n');
  const second = server.handleRequest(request);

  assert.match(first.transformed[0].text, /"dependency-one"/);
  assert.match(second.transformed[0].text, /"dependency-two"/);
  server.close();
});

test("plugin console output is scoped to the response that produced it", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-logs-"));
  const sourceFile = path.join(fixtureDir, "main.ts");
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(sourceFile, "export {};\n");
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `module.exports = (program, config) => {
  console.error(config.message);
  return () => (source) => source;
};\n`);
  fs.writeFileSync(configPath, JSON.stringify({ compilerOptions: { noLib: true }, files: ["main.ts"] }));
  const server = createProtocolServer(ts);
  const request = {
    protocol: 2,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
    fileNames: [sourceFile],
    compileFileNames: [sourceFile],
    changedFiles: [],
  };

  const first = server.handleRequest({ ...request, plugins: [{ transform: "./plugin.js", message: "first request" }] });
  const second = server.handleRequest({ ...request, plugins: [{ transform: "./plugin.js", message: "second request" }] });

  assert.deepEqual(first.logs, ["first request"]);
  assert.deepEqual(second.logs, ["second request"]);
  server.close();
});

test("warm builds a program without loading or invoking transformer factories", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-warm-"));
  const sourceFile = path.join(fixtureDir, "main.ts");
  const markerFile = path.join(fixtureDir, "factory-loaded");
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(sourceFile, "export {};\n");
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `require("node:fs").writeFileSync(${JSON.stringify(markerFile)}, "loaded");
module.exports = () => () => (source) => source;\n`);
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: { noLib: true, plugins: [{ transform: "./plugin.js" }] },
    files: ["main.ts"],
  }));
  const server = createProtocolServer(ts);

  const response = server.handleRequest({
    protocol: 2,
    operation: "warm",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
  });

  assert.deepEqual(response.diagnostics, []);
  assert.equal(fs.existsSync(markerFile), false);
  server.close();
});

test("a cold transform retries after the disk returns to its compiler snapshot", (t) => {
  const originalCwd = process.cwd();
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-cold-race-"));
  t.after(() => {
    process.chdir(originalCwd);
    fs.rmSync(fixtureDir, { recursive: true, force: true });
  });
  const sourceFile = path.join(fixtureDir, "main.ts");
  const configPath = path.join(fixtureDir, "tsconfig.json");
  const expectedText = 'export const phase = "expected";\n';
  const racedText = 'export const phase = "disk-now";\n';
  fs.writeFileSync(sourceFile, racedText);
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `module.exports = (program, config, helpers) => (context) => {
  const visit = (node) => helpers.ts.isStringLiteral(node)
    ? helpers.ts.factory.createStringLiteral(node.text)
    : helpers.ts.visitEachChild(node, visit, context);
  return (source) => helpers.ts.visitNode(source, visit);
};\n`);
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: { noLib: true, plugins: [{ transform: "./plugin.js" }] },
    files: ["main.ts"],
  }));
  const server = createProtocolServer(ts);
  const request = {
    protocol: 2,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
    fileNames: [sourceFile],
    compileFileNames: [sourceFile],
    fileContentIdentities: { [sourceFile]: contentIdentity(expectedText) },
    changedFiles: [],
  };

  const raced = server.handleRequest(request);
  fs.writeFileSync(sourceFile, expectedText);
  const retried = server.handleRequest(request);

  assert.deepEqual(raced.transformed, []);
  assert.match(raced.diagnostics[0].message, /source changed after the compiler snapshot/);
  assert.deepEqual(retried.diagnostics, []);
  assert.match(retried.transformed[0].text, /"expected"/);
  server.close();
});

test("the first transform refreshes a disk edit made after warm", (t) => {
  const originalCwd = process.cwd();
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-warm-edit-"));
  t.after(() => {
    process.chdir(originalCwd);
    fs.rmSync(fixtureDir, { recursive: true, force: true });
  });
  const sourceFile = path.join(fixtureDir, "main.ts");
  const configPath = path.join(fixtureDir, "tsconfig.json");
  const beforeText = 'export const phase = "before";\n';
  const afterText = 'export const phase = "second";\n';
  const racedText = 'export const phase = "third!";\n';
  fs.writeFileSync(sourceFile, beforeText);
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `module.exports = (program, config, helpers) => (context) => {
  const visit = (node) => helpers.ts.isStringLiteral(node)
    ? helpers.ts.factory.createStringLiteral(node.text)
    : helpers.ts.visitEachChild(node, visit, context);
  return (source) => helpers.ts.visitNode(source, visit);
};\n`);
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: { noLib: true, plugins: [{ transform: "./plugin.js" }] },
    files: ["main.ts"],
  }));
  const server = createProtocolServer(ts);
  const identity = { protocol: 2, projectDir: fixtureDir, tsConfigPath: configPath };

  const warm = server.handleRequest({ ...identity, operation: "warm" });
  const beforeRewrite = fs.statSync(sourceFile);
  fs.writeFileSync(sourceFile, racedText);
  fs.utimesSync(sourceFile, beforeRewrite.atime, beforeRewrite.mtime);
  const transform = {
    ...identity,
    operation: "transform",
    fileNames: [sourceFile],
    compileFileNames: [sourceFile],
    fileContentIdentities: {
      [sourceFile]: crypto.createHash("sha256").update(afterText).digest("hex"),
    },
    changedFiles: [],
  };
  const raced = server.handleRequest(transform);
  fs.writeFileSync(sourceFile, afterText);
  fs.utimesSync(sourceFile, beforeRewrite.atime, beforeRewrite.mtime);
  const transformed = server.handleRequest(transform);

  assert.deepEqual(warm.diagnostics, []);
  assert.deepEqual(raced.transformed, []);
  assert.match(raced.diagnostics[0].message, /source changed after the compiler snapshot/);
  assert.deepEqual(transformed.diagnostics, []);
  assert.match(transformed.transformed[0].text, /"second"/);
  server.close();
});

test("editing the loaded TypeScript runtime is visible on the next transform", () => {
  const fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-ts-freshness-"));
  const sourceFile = path.join(fixtureDir, "main.ts");
  const runtimePath = path.join(fixtureDir, "typescript-runtime.js");
  const configPath = path.join(fixtureDir, "tsconfig.json");
  fs.writeFileSync(sourceFile, 'export const phase = "input";\n');
  fs.writeFileSync(runtimePath, 'module.exports = "runtime-one";\n');
  fs.writeFileSync(path.join(fixtureDir, "plugin.js"), `module.exports = (program, config, helpers) => (context) => {
  const visit = (node) => helpers.ts.isStringLiteral(node)
    ? helpers.ts.factory.createStringLiteral(helpers.ts.runtimeLabel)
    : helpers.ts.visitEachChild(node, visit, context);
  return (source) => helpers.ts.visitNode(source, visit);
};\n`);
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: { noLib: true, plugins: [{ transform: "./plugin.js" }] },
    files: ["main.ts"],
  }));
  const loadTypeScript = () => {
    const api = Object.create(ts);
    api.runtimeLabel = require(runtimePath);
    return api;
  };
  loadTypeScript.modulePathFor = () => fs.realpathSync(runtimePath);
  const server = createProtocolServer(loadTypeScript);
  const request = {
    protocol: 2,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
    fileNames: [sourceFile],
    compileFileNames: [sourceFile],
    changedFiles: [],
  };

  const first = server.handleRequest(request);
  fs.writeFileSync(runtimePath, 'module.exports = "runtime-two";\n');
  const second = server.handleRequest(request);

  assert.match(first.transformed[0].text, /"runtime-one"/);
  assert.match(second.transformed[0].text, /"runtime-two"/);
  server.close();
});

test("plugin cwd uses the session's canonical project path", (t) => {
  const originalCwd = process.cwd();
  const realProjectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-cwd-real-"));
  const aliasProjectDir = `${realProjectDir}-alias`;
  t.after(() => {
    process.chdir(originalCwd);
    fs.rmSync(aliasProjectDir, { force: true });
    fs.rmSync(realProjectDir, { recursive: true, force: true });
  });
  try {
    fs.symlinkSync(realProjectDir, aliasProjectDir, "dir");
  } catch (error) {
    t.skip(`cannot create project-path symlink: ${error.message}`);
    return;
  }
  const sourceFile = path.join(realProjectDir, "main.ts");
  const configPath = path.join(realProjectDir, "tsconfig.json");
  fs.writeFileSync(sourceFile, 'export const phase = "input";\n');
  fs.writeFileSync(path.join(realProjectDir, "plugin.js"), `const fs = require("node:fs");
const path = require("node:path");
module.exports = (program, config, helpers) => (context) => {
  const projectIndex = Math.max(process.argv.indexOf("-p"), process.argv.indexOf("--project"));
  const configPath = process.argv[projectIndex + 1];
  const usesCanonicalProject = fs.realpathSync(process.cwd()) === fs.realpathSync(path.dirname(configPath));
  const visit = (node) => helpers.ts.isStringLiteral(node)
    ? helpers.ts.factory.createStringLiteral(usesCanonicalProject ? "canonical-project" : "split-project")
    : helpers.ts.visitEachChild(node, visit, context);
  return (source) => helpers.ts.visitNode(source, visit);
};\n`);
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: { noLib: true, plugins: [{ transform: "./plugin.js" }] },
    files: ["main.ts"],
  }));
  const originalArgv = process.argv.slice();
  t.after(() => process.argv.splice(0, process.argv.length, ...originalArgv));
  process.argv.push("--project", path.join(aliasProjectDir, "tsconfig.json"));
  const server = createProtocolServer(ts);
  t.after(() => server.close());
  const response = server.handleRequest({
    protocol: 2,
    operation: "transform",
    projectDir: aliasProjectDir,
    tsConfigPath: path.join(aliasProjectDir, "tsconfig.json"),
    fileNames: [path.join(aliasProjectDir, "main.ts")],
    compileFileNames: [path.join(aliasProjectDir, "main.ts")],
    changedFiles: [],
  });

  assert.deepEqual(response.diagnostics, []);
  assert.match(response.transformed[0].text, /"canonical-project"/);
});
