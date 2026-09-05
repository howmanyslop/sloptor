const assert = require("node:assert/strict");
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

// TypeScript reports file names with forward slashes on every platform, so
// comparisons against host paths must be slash-normalized to hold on Windows.
function toSlash(value) {
  try {
    return fs.realpathSync.native?.(value).replace(/\\/g, "/") ?? fs.realpathSync(value).replace(/\\/g, "/");
  } catch {
    return value.replace(/\\/g, "/");
  }
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
    protocol: 1,
    operation: "transform",
    projectDir,
    tsConfigPath: configPath,
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
    protocol: 1,
    operation: "transform",
    projectDir,
    tsConfigPath,
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
    protocol: 1,
    operation: "transform",
    projectDir,
    tsConfigPath: configPath,
    compileFileNames: [sourcePath],
    changedFiles: [],
  });

  assert.equal(response.afterDeclarationsTransformers, 0);
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
  const server = new sidecar.SidecarServer(ts);
  const response = server.handleRequest({
    protocol: 1,
    projectDir,
    tsConfigPath,
    compileFileNames: [sourcePath],
    changedFiles: [],
  });

  assert.deepEqual(response.transformed, []);
  assert.equal(response.diagnostics.length, 1);
  assert.equal(response.diagnostics[0].code, "invalid-request");
  assert.equal(response.diagnostics[0].message, 'operation must equal "transform" or "validate"');
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
    protocol: 1,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
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
    input: `${JSON.stringify({
      protocol: 1,
      operation: "transform",
      projectDir: aliasDir,
      tsConfigPath: aliasConfigPath,
      compileFileNames: [path.join(aliasDir, "src", "main.ts")],
      changedFiles: [],
    })}\n`,
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
    protocol: 1,
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
    protocol: 1,
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
    protocol: 1,
    operation: "transform",
    projectDir: fixtureDir,
    tsConfigPath: configPath,
    compileFileNames: [fileName],
    changedFiles: [],
  });

  assert.deepEqual(response.diagnostics, []);
  assert.deepEqual(response.metrics.plugins.map((plugin) => plugin.ms >= 20), [true, true]);
});

test("sidecar protocol responses include optional request metrics", () => {
  const server = new sidecar.SidecarServer(ts);
  const response = server.handleRequest({
    protocol: 1,
    operation: "transform",
    projectDir,
    tsConfigPath,
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
  const server = new sidecar.SidecarServer(ts);
  const response = server.handleRequest({
    protocol: 1,
    operation: "transform",
    projectDir,
    tsConfigPath,
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
    child.stdin.write(`${JSON.stringify({
      protocol: 1,
    operation: "transform",
      projectDir,
      tsConfigPath,
      compileFileNames: [sourcePath],
      changedFiles: [],
    })}\n`);

    const firstResponse = await firstResponsePromise;
    assert.deepEqual(firstResponse.diagnostics, []);
    assert.equal(firstResponse.transformed.length, 1);
    assert.match(firstResponse.transformed[0].text, /after:before:start/);

    const secondResponsePromise = readProtocolLine(child.stdout);
    child.stdin.write(`${JSON.stringify({
      protocol: 1,
    operation: "transform",
      projectDir,
      tsConfigPath,
      compileFileNames: [sourcePath],
      changedFiles: [
        {
          fileName: sourcePath,
          text: 'export const phase = "memory";\n',
        },
      ],
    })}\n`);

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
    input: `${JSON.stringify({
      protocol: 1,
    operation: "transform",
      projectDir: hookProjectDir,
      tsConfigPath: path.join(hookProjectDir, "tsconfig.json"),
      compileFileNames: [selectedPath, skippedPath],
      changedFiles: [],
    })}\n`,
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

  const request = JSON.stringify({
    protocol: 1,
    operation: "transform",
    tsConfigPath: path.join(noisyProjectDir, "tsconfig.json"),
    projectDir: noisyProjectDir,
    compileFileNames: [noisyMainFile],
    changedFiles: [],
  });

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
  assert.match(result.stderr, /plugin chatter on stdout/);
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
    protocol: 1,
    operation: "transform",
    projectDir: project.projectRoot,
    tsConfigPath: project.configPath,
    compileFileNames: rootFileNames,
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

// A watch session reuses one warm worker, so a narrowed root set has to be
// stable across rebuilds: it only ever grows, and a request that sends no
// limit widens it back to the whole project for good.
test("the worker's root limit only ever widens within a session", () => {
  const project = narrowRootsProject();
  const session = new sidecar.SidecarProjectSession(ts, project.projectRoot, project.configPath);

  session.handleRequest(narrowRootsRequest(project, [project.files.a]));
  const afterFirst = session.projectVersion;
  session.handleRequest(narrowRootsRequest(project, [project.files.b]));
  assert.deepEqual(programFileNames(session), [toSlash(project.files.a), toSlash(project.files.b)]);

  const afterSecond = session.projectVersion;
  session.handleRequest(narrowRootsRequest(project, [project.files.a]));
  assert.equal(session.projectVersion, afterSecond, "a subset request must not disturb the program");
  assert.ok(afterSecond > afterFirst, "widening the root set must invalidate the program");

  const full = narrowRootsRequest(project, [project.files.c]);
  delete full.rootFileNames;
  session.handleRequest(full);
  assert.deepEqual(programFileNames(session), [
    toSlash(project.files.a),
    toSlash(project.files.b),
    toSlash(project.files.c),
    toSlash(project.files.globals),
  ]);

  session.handleRequest(narrowRootsRequest(project, [project.files.a]));
  assert.equal(session.rootLimit, undefined, "a full build must switch narrowing off for the session");
});

test("rootFileNames must be an array of strings when present", () => {
  const project = narrowRootsProject();
  const server = new sidecar.SidecarServer(ts);
  const response = server.handleRequest({ ...narrowRootsRequest(project, [project.files.a]), rootFileNames: [1] });

  assert.equal(response.diagnostics.length, 1);
  assert.match(response.diagnostics[0].message, /rootFileNames must be an array of strings/);
});

// The root limit never shrinks, so a file that leaves the project stays in it.
// getScriptFileNames drops what is not on disk, so the stale root costs the
// program nothing and never reaches TypeScript as a missing root.
test("a narrowed root whose file was deleted is dropped, not reported", () => {
  const project = narrowRootsProject();
  const session = new sidecar.SidecarProjectSession(ts, project.projectRoot, project.configPath);

  session.handleRequest(narrowRootsRequest(project, [project.files.a, project.files.b]));
  assert.deepEqual(programFileNames(session), [toSlash(project.files.a), toSlash(project.files.b)]);

  fs.rmSync(project.files.b);
  const response = session.handleRequest(narrowRootsRequest(project, [project.files.a]));

  assert.deepEqual(response.diagnostics, []);
  assert.ok(session.rootLimit.has(session.canonicalize(project.files.b)), "the limit should still name the deleted file");
  assert.deepEqual(programFileNames(session), [toSlash(project.files.a)]);
});

// The root limit is monotonic, and a limitless request is its ceiling: after
// one, the effective root set is every file the tsconfig names and no later
// narrowed request may shrink it — nor disturb the program TypeScript built for
// it.
test("a narrowed request after a limitless one neither shrinks nor churns the program", () => {
  const project = narrowRootsProject();
  const session = new sidecar.SidecarProjectSession(ts, project.projectRoot, project.configPath);
  const everything = [
    toSlash(project.files.a),
    toSlash(project.files.b),
    toSlash(project.files.c),
    toSlash(project.files.globals),
  ];

  session.handleRequest(narrowRootsRequest(project, [project.files.a]));
  assert.deepEqual(programFileNames(session), [toSlash(project.files.a)]);

  const full = narrowRootsRequest(project, [project.files.a]);
  delete full.rootFileNames;
  session.handleRequest(full);
  assert.deepEqual(programFileNames(session), everything);

  const settled = session.projectVersion;
  const response = session.handleRequest(narrowRootsRequest(project, [project.files.b]));

  assert.deepEqual(response.diagnostics, []);
  assert.equal(session.rootLimit, undefined, "the limit is already the whole project");
  assert.deepEqual(programFileNames(session), everything, "a narrowed request must not shrink the root set");
  assert.equal(session.projectVersion, settled, "a narrowed request must not invalidate the program");
});
