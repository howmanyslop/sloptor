const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const ts = require("typescript");

const { SidecarProjectSession, SidecarServer } = require("../index.js");

function digest(fileName) {
  return crypto.createHash("sha256").update(fs.readFileSync(fileName)).digest("hex");
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

function transformRequest(project, fileNames, compileFileNames = fileNames, changedFiles = []) {
  return {
    protocol: 3,
    operation: "transform",
    projectDir: project.requestRoot,
    tsConfigPath: project.requestConfig,
    configSnapshot: captureConfigSnapshot(project.configPath),
    fileNames,
    compileFileNames,
    rootFileNames: fileNames,
    changedFiles,
    fileContentIdentities: Object.fromEntries(fileNames.map((fileName) => [fileName, digest(fileName)])),
  };
}

function makeProject(t, files, pluginSource) {
  const originalCwd = process.cwd();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-retention-"));
  const sourceDir = path.join(root, "src");
  fs.mkdirSync(sourceDir);
  for (const [name, text] of Object.entries(files)) {
    fs.writeFileSync(path.join(sourceDir, name), text);
  }
  if (pluginSource) {
    fs.writeFileSync(path.join(root, "plugin.js"), pluginSource);
  }
  const configPath = path.join(root, "tsconfig.json");
  const config = {
    compilerOptions: {
      module: "CommonJS",
      moduleResolution: "Node",
      noLib: true,
      target: "ESNext",
      plugins: pluginSource ? [{ transform: "./plugin.js" }] : [],
    },
    include: ["src"],
  };
  fs.writeFileSync(configPath, JSON.stringify(config));
  t.after(() => {
    process.chdir(originalCwd);
    fs.rmSync(root, { recursive: true, force: true });
  });
  return {
    root,
    config,
    configPath,
    requestRoot: root,
    requestConfig: configPath,
    sourceDir,
  };
}

function assertNoDiagnostics(response) {
  assert.deepEqual(response.diagnostics, []);
}

// Catches an editor deleting a symlinked source after a transform: its trace
// handle must still map that source, and recreating it must begin a new plugin
// lifetime. The expected markers are literals emitted by this fixture plugin.
test("retained maps survive symlink deletion and recreation gets a fresh transformer", (t) => {
  const project = makeProject(t, { "main.ts": "export const value = 1;\n" }, `module.exports = () => {
  let visits = 0;
  return (context) => (sourceFile) => context.factory.updateSourceFile(sourceFile, [
    ...sourceFile.statements,
    context.factory.createExpressionStatement(context.factory.createStringLiteral("visit:" + ++visits)),
  ]);
};\n`);
  const aliasRoot = `${project.root}-alias`;
  t.after(() => fs.rmSync(aliasRoot, { force: true }));
  try {
    fs.symlinkSync(project.root, aliasRoot, process.platform === "win32" ? "junction" : "dir");
  } catch (error) {
    if (error?.code === "EPERM" || error?.code === "EACCES") {
      t.skip(`cannot create project-path link: ${error.message}`);
      return;
    }
    throw error;
  }

  project.requestRoot = aliasRoot;
  project.requestConfig = path.join(aliasRoot, "tsconfig.json");
  const aliasMain = path.join(aliasRoot, "src", "main.ts");
  const physicalMain = path.join(project.sourceDir, "main.ts");
  const session = new SidecarProjectSession(ts, aliasRoot, project.requestConfig);
  t.after(() => session.dispose());

  const first = session.handleRequest(transformRequest(project, [aliasMain]));
  assertNoDiagnostics(first);
  assert.equal(typeof first.resultHandle, "string");

  fs.rmSync(physicalMain);
  const deleted = session.handleRequest(transformRequest(project, [], [], [{ fileName: aliasMain, deleted: true }]));
  assert.equal(deleted.diagnostics[0]?.code, "18003");

  const maps = session.handleRequest({ operation: "maps", resultHandle: first.resultHandle, fileNames: [aliasMain] });
  assertNoDiagnostics(maps);
  assert.equal(maps.traceMaps.length, 1);
  assert.doesNotThrow(() => JSON.parse(maps.traceMaps[0].traceMap));

  const release = session.handleRequest({ operation: "release", resultHandle: first.resultHandle, outcome: "success" });
  assertNoDiagnostics(release);

  fs.writeFileSync(physicalMain, "export const value = 2;\n");
  const recreated = session.handleRequest(transformRequest(project, [aliasMain]));
  assertNoDiagnostics(recreated);
  assert.match(recreated.transformed[0].text, /"visit:1"/);
});

test("a retargeted source link uses its current source after a result release", (t) => {
  const originalCwd = process.cwd();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-retarget-"));
  const sourceLink = path.join(root, "src");
  const firstTarget = path.join(root, "first");
  const secondTarget = path.join(root, "second");
  const configPath = path.join(root, "tsconfig.json");
  const sourcePath = path.join(sourceLink, "main.ts");
  let server;
  t.after(() => {
    server?.close();
    process.chdir(originalCwd);
    fs.rmSync(root, { recursive: true, force: true });
  });
  fs.mkdirSync(firstTarget);
  fs.mkdirSync(secondTarget);
  fs.writeFileSync(path.join(firstTarget, "main.ts"), 'export const value = "first";\n');
  fs.writeFileSync(path.join(secondTarget, "main.ts"), 'export const value = "second";\n');
  fs.writeFileSync(path.join(root, "plugin.js"), `module.exports = () => (context) => (sourceFile) => context.factory.updateSourceFile(sourceFile, [
  ...sourceFile.statements,
  context.factory.createExpressionStatement(context.factory.createStringLiteral("changed")),
]);\n`);
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: {
      module: "CommonJS",
      moduleResolution: "Node",
      noLib: true,
      target: "ESNext",
      plugins: [{ transform: "./plugin.js" }],
    },
    files: ["src/main.ts"],
  }));
  try {
    fs.symlinkSync(firstTarget, sourceLink, process.platform === "win32" ? "junction" : "dir");
  } catch (error) {
    if (error?.code === "EPERM" || error?.code === "EACCES") {
      t.skip(`cannot create source link: ${error.message}`);
      return;
    }
    throw error;
  }

  const project = { requestRoot: root, requestConfig: configPath, configPath };
  server = new SidecarServer(ts);
  const first = server.handleRequest(transformRequest(project, [sourcePath]));
  assertNoDiagnostics(first);
  assert.equal(typeof first.resultHandle, "string");
  assertNoDiagnostics(server.handleRequest({
    protocol: 3,
    operation: "release",
    projectDir: root,
    tsConfigPath: configPath,
    resultHandle: first.resultHandle,
    outcome: "success",
  }));

  fs.rmSync(sourceLink, { recursive: true, force: true });
  fs.symlinkSync(secondTarget, sourceLink, process.platform === "win32" ? "junction" : "dir");
  const second = server.handleRequest(transformRequest(project, [sourcePath]));
  assertNoDiagnostics(second);
  const transformedSource = fs.statSync(second.transformed[0].fileName);
  const currentSource = fs.statSync(sourcePath);
  assert.deepEqual(
    { device: transformedSource.dev, inode: transformedSource.ino },
    { device: currentSource.dev, inode: currentSource.ino },
  );
  assert.match(second.transformed[0].text, /value = "second"/);
});

test("a regular source replaced by a link uses the replacement source", (t) => {
  const originalCwd = process.cwd();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-replaced-source-"));
  const sourceDir = path.join(root, "src");
  const replacementDir = path.join(root, "replacement");
  const configPath = path.join(root, "tsconfig.json");
  const sourcePath = path.join(sourceDir, "main.ts");
  let server;
  t.after(() => {
    server?.close();
    process.chdir(originalCwd);
    fs.rmSync(root, { recursive: true, force: true });
  });
  fs.mkdirSync(sourceDir);
  fs.mkdirSync(replacementDir);
  fs.writeFileSync(sourcePath, 'export const value = "first";\n');
  fs.writeFileSync(path.join(replacementDir, "main.ts"), 'export const value = "replacement";\n');
  fs.writeFileSync(path.join(root, "plugin.js"), `module.exports = () => (context) => (sourceFile) => context.factory.updateSourceFile(sourceFile, [
  ...sourceFile.statements,
  context.factory.createExpressionStatement(context.factory.createStringLiteral("changed")),
]);\n`);
  fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: {
      module: "CommonJS",
      moduleResolution: "Node",
      noLib: true,
      target: "ESNext",
      plugins: [{ transform: "./plugin.js" }],
    },
    files: ["src/main.ts"],
  }));
  const project = { requestRoot: root, requestConfig: configPath, configPath };
  server = new SidecarServer(ts);
  const first = server.handleRequest(transformRequest(project, [sourcePath]));
  assertNoDiagnostics(first);
  assert.equal(typeof first.resultHandle, "string");
  assertNoDiagnostics(server.handleRequest({
    protocol: 3,
    operation: "release",
    projectDir: root,
    tsConfigPath: configPath,
    resultHandle: first.resultHandle,
    outcome: "success",
  }));

  fs.rmSync(sourceDir, { recursive: true, force: true });
  try {
    fs.symlinkSync(replacementDir, sourceDir, process.platform === "win32" ? "junction" : "dir");
  } catch (error) {
    if (error?.code === "EPERM" || error?.code === "EACCES") {
      t.skip(`cannot replace source with a link: ${error.message}`);
      return;
    }
    throw error;
  }
  const replacement = server.handleRequest(transformRequest(project, [sourcePath]));
  assertNoDiagnostics(replacement);
  const transformedSource = fs.statSync(replacement.transformed[0].fileName);
  const currentSource = fs.statSync(sourcePath);
  assert.deepEqual(
    { device: transformedSource.dev, inode: transformedSource.ino },
    { device: currentSource.dev, inode: currentSource.ino },
  );
  assert.match(replacement.transformed[0].text, /value = "replacement"/);
});

test("a config snapshot that removes a root removes it from later transformer programs", (t) => {
  const project = makeProject(t, {
    "a.ts": "export const a = 1;\n",
    "b.ts": "export const b = 2;\n",
  });
  const a = path.join(project.sourceDir, "a.ts");
  const b = path.join(project.sourceDir, "b.ts");
  const physicalB = fs.realpathSync.native?.(b) ?? fs.realpathSync(b);
  const pluginPath = path.join(project.root, "plugin.js");
  fs.writeFileSync(pluginPath, `module.exports = (program, config, helpers) => (context) => (sourceFile) => context.factory.updateSourceFile(sourceFile, [
  ...sourceFile.statements,
  context.factory.createExpressionStatement(context.factory.createStringLiteral(program.getSourceFile(${JSON.stringify(physicalB)}) ? "stale-root" : "current-root")),
]);\n`);
  project.config.compilerOptions.plugins = [{ transform: "./plugin.js" }];
  project.config.include = undefined;
  project.config.files = ["src/a.ts", "src/b.ts"];
  fs.writeFileSync(project.configPath, JSON.stringify(project.config));
  const session = new SidecarProjectSession(ts, project.root, project.configPath);
  t.after(() => session.dispose());

  const first = session.handleRequest(transformRequest(project, [a, b], [a]));
  assertNoDiagnostics(first);
  assert.match(first.transformed[0].text, /"stale-root"/);

  project.config.files = ["src/a.ts"];
  fs.writeFileSync(project.configPath, JSON.stringify(project.config));
  const second = session.handleRequest(transformRequest(project, [a]));
  assertNoDiagnostics(second);
  assert.match(second.transformed[0].text, /"current-root"/);
});

// Resource acceptance: complete source snapshots must not leave one metadata
// entry per deleted source in a daemon session. This measures metadata only;
// it intentionally makes no claim about RSS or garbage collection.
test("complete source snapshots discard metadata for deleted transient files", (t) => {
  const project = makeProject(t, { "main.ts": "export const main = 1;\n" });
  const main = path.join(project.sourceDir, "main.ts");
  const session = new SidecarProjectSession(ts, project.root, project.configPath);
  t.after(() => session.dispose());

  for (let index = 0; index < 100; index += 1) {
    const transient = path.join(project.sourceDir, `transient-${index}.ts`);
    fs.writeFileSync(transient, `export const value${index} = ${index};\n`);
    assertNoDiagnostics(session.handleRequest(transformRequest(project, [main, transient], [transient], [{ fileName: transient, text: fs.readFileSync(transient, "utf8") }])));

    fs.rmSync(transient);
    assertNoDiagnostics(session.handleRequest(transformRequest(project, [main], [main], [{ fileName: transient, deleted: true }])));
  }

  const historical = /transient-\d+\.ts$/;
  assert.equal([...session.actualPaths.values()].some((fileName) => historical.test(fileName)), false);
  assert.equal([...session.pathAliases.values()].some((fileName) => historical.test(fileName)), false);
  assert.equal([...session.versions.keys()].some((fileName) => historical.test(fileName)), false);
  assert.equal([...session.baseRoots.values()].some((fileName) => historical.test(fileName)), false);
  assert.equal([...session.rootLimit?.values() ?? []].some((fileName) => historical.test(fileName)), false);
  assert.equal(session.deleted.size, 0);
});
