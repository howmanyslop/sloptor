const assert = require("node:assert/strict");
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { pathToFileURL } = require("node:url");

const sidecarDir = path.resolve(__dirname, "..");
const typescriptPath = require.resolve("typescript");
const packageMapSupported = process.allowedNodeEnvironmentFlags.has("--experimental-package-map");

function createProject(t) {
  const workspaceDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-package-map-"));
  t.after(() => fs.rmSync(workspaceDir, { recursive: true, force: true }));
  const projectDir = path.join(workspaceDir, "packages", "logger");
  fs.mkdirSync(projectDir, { recursive: true });
  const packages = {
    workspace: {
      url: pathToFileURL(workspaceDir).href,
      dependencies: { typescript: "typescript" },
    },
    project: { url: pathToFileURL(projectDir).href },
    sidecar: { url: pathToFileURL(sidecarDir).href },
    typescript: { url: pathToFileURL(path.dirname(path.dirname(typescriptPath))).href },
  };
  return { workspaceDir, projectDir, packages };
}

function runWithPackageMap(project, script, { nodeOptions = false, quoteFlag = false, flagName = "--experimental-package-map" } = {}) {
  const mapPath = path.join(project.workspaceDir, "package-map.json");
  fs.writeFileSync(mapPath, JSON.stringify({ packages: project.packages }));
  const flag = `${flagName}=${mapPath}`;
  const envPath = mapPath.replace(/\\/g, "/");
  const envFlag = quoteFlag ? `"${flagName}=${envPath}"` : `${flagName}="${envPath}"`;
  const result = spawnSync(process.execPath, [
    ...(nodeOptions ? [] : [flag]),
    "--eval",
    `const assert = require("node:assert/strict");
const sidecar = require(${JSON.stringify(path.join(sidecarDir, "index.js"))});
const projectDir = ${JSON.stringify(project.projectDir)};
${script}`,
  ], {
    cwd: project.workspaceDir,
    env: { ...process.env, NODE_OPTIONS: nodeOptions ? envFlag : "" },
    encoding: "utf8",
    timeout: 30_000,
    windowsHide: true,
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr || result.stdout);
}

function addPlugin(project) {
  const pluginDir = path.join(project.workspaceDir, "vendor", "prefix-transformer");
  fs.mkdirSync(pluginDir, { recursive: true });
  fs.writeFileSync(path.join(pluginDir, "package.json"), JSON.stringify({ main: "index.js" }));
  fs.copyFileSync(path.join(sidecarDir, "testdata", "project", "plugins", "prefix-string.js"), path.join(pluginDir, "index.js"));
  project.packages.workspace.dependencies["prefix-transformer"] = "plugin";
  project.packages.plugin = {
    url: pathToFileURL(pluginDir).href,
    dependencies: { typescript: "typescript" },
  };
  fs.writeFileSync(path.join(project.projectDir, "main.ts"), 'export const message = "hello";\n');
  fs.writeFileSync(path.join(project.projectDir, "tsconfig.json"), JSON.stringify({
    compilerOptions: {
      noLib: true,
      plugins: [{ transform: "prefix-transformer", type: "config", import: "configTransformer", prefix: "mapped" }],
    },
    files: ["main.ts"],
  }));
}

test("resolveTypeScript finds a workspace compiler under a package map", { skip: !packageMapSupported }, (t) => {
  const project = createProject(t);
  runWithPackageMap(project, `
assert.ok(sidecar.resolveTypeScript(projectDir) === require(${JSON.stringify(typescriptPath)}), "expected the project compiler");
`);
});

test("resolveTypeScript prefers the mapped project compiler over the sidecar's compiler", { skip: !packageMapSupported }, (t) => {
  const project = createProject(t);
  const fallbackDir = path.join(project.workspaceDir, "fallback-typescript");
  fs.mkdirSync(fallbackDir);
  fs.writeFileSync(path.join(fallbackDir, "package.json"), JSON.stringify({ main: "index.js" }));
  fs.writeFileSync(path.join(fallbackDir, "index.js"), "module.exports = {};\n");
  project.packages.fallback = { url: pathToFileURL(fallbackDir).href };
  project.packages.sidecar.dependencies = { typescript: "fallback" };
  runWithPackageMap(project, `
assert.ok(sidecar.resolveTypeScript(projectDir) === require(${JSON.stringify(typescriptPath)}), "expected the project compiler");
`);
});

test("SidecarServer runs a workspace transformer with a package map in NODE_OPTIONS", { skip: !packageMapSupported }, (t) => {
  const project = createProject(t);
  addPlugin(project);
  runWithPackageMap(project, `
const path = require("node:path");
const server = new sidecar.SidecarServer(sidecar.resolveTypeScript);
try {
  const response = server.handleRequest({
    protocol: 1,
    operation: "transform",
    projectDir,
    tsConfigPath: path.join(projectDir, "tsconfig.json"),
    compileFileNames: [path.join(projectDir, "main.ts")],
    changedFiles: [],
  });
  assert.deepEqual(response.diagnostics, []);
  assert.match(response.transformed[0].text, /mapped:hello/);
} finally {
  server.dispose();
}
`, { nodeOptions: true });
});

test("resolveTypeScript accepts a fully quoted package-map option in NODE_OPTIONS", { skip: !packageMapSupported }, (t) => {
  const project = createProject(t);
  runWithPackageMap(project, `
assert.ok(sidecar.resolveTypeScript(projectDir) === require(${JSON.stringify(typescriptPath)}), "expected the project compiler");
`, { nodeOptions: true, quoteFlag: true });
});

for (const nodeOptions of [false, true]) {
  test(`resolveTypeScript accepts the package-map underscore alias in ${nodeOptions ? "NODE_OPTIONS" : "execArgv"}`, { skip: !packageMapSupported }, (t) => {
    const project = createProject(t);
    runWithPackageMap(project, `
assert.ok(sidecar.resolveTypeScript(projectDir) === require(${JSON.stringify(typescriptPath)}), "expected the project compiler");
`, { nodeOptions, flagName: "--experimental_package_map" });
  });
}

test("SidecarServer rejects a transformer with a different mapped TypeScript instance", { skip: !packageMapSupported }, (t) => {
  const project = createProject(t);
  addPlugin(project);
  const otherTypeScriptDir = path.join(project.workspaceDir, "vendor", "other-typescript");
  fs.mkdirSync(otherTypeScriptDir, { recursive: true });
  fs.writeFileSync(path.join(otherTypeScriptDir, "package.json"), JSON.stringify({ main: "index.js" }));
  fs.writeFileSync(path.join(otherTypeScriptDir, "index.js"), "module.exports = {};\n");
  project.packages.otherTypeScript = { url: pathToFileURL(otherTypeScriptDir).href };
  project.packages.plugin.dependencies.typescript = "otherTypeScript";
  runWithPackageMap(project, `
const path = require("node:path");
const server = new sidecar.SidecarServer(sidecar.resolveTypeScript);
try {
  const response = server.handleRequest({
    protocol: 1,
    operation: "validate",
    projectDir,
    tsConfigPath: path.join(projectDir, "tsconfig.json"),
  });
  assert.deepEqual(response.diagnostics.map((diagnostic) => diagnostic.code), ["typescript-instance-mismatch"]);
} finally {
  server.dispose();
}
`);
});

test("SidecarServer reports a missing relative transformer even when an ancestor has that file", { skip: !packageMapSupported }, (t) => {
  const project = createProject(t);
  addPlugin(project);
  fs.copyFileSync(path.join(sidecarDir, "testdata", "project", "plugins", "prefix-string.js"), path.join(project.workspaceDir, "plugin.js"));
  runWithPackageMap(project, `
const path = require("node:path");
const server = new sidecar.SidecarServer(sidecar.resolveTypeScript);
try {
  const response = server.handleRequest({
    protocol: 1,
    operation: "validate",
    projectDir,
    tsConfigPath: path.join(projectDir, "tsconfig.json"),
    plugins: [{ transform: "./plugin.js" }],
  });
  assert.deepEqual(response.diagnostics.map((diagnostic) => diagnostic.code), ["transformer-not-found"]);
} finally {
  server.dispose();
}
`);
});
