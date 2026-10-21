const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { packageDigest, preflightParallelTransformers } = require("../lib/parallel");

function project(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-parallel-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const plugin = path.join(dir, "node_modules", "reviewed-plugin");
  const typescript = path.join(dir, "node_modules", "typescript");
  fs.mkdirSync(plugin, { recursive: true });
  fs.mkdirSync(typescript, { recursive: true });
  fs.writeFileSync(path.join(plugin, "package.json"), JSON.stringify({ name: "reviewed-plugin", main: "index.js" }));
  fs.writeFileSync(path.join(plugin, "index.js"), "exports.default = () => { throw new Error('FACTORY MUST NOT RUN'); };\n");
  fs.writeFileSync(path.join(typescript, "package.json"), JSON.stringify({ name: "typescript", main: "index.js" }));
  fs.writeFileSync(path.join(typescript, "index.js"), "module.exports = { version: 'reviewed' };\n");
  const config = { transform: "reviewed-plugin", silent: true };
  const entry = path.join(plugin, "index.js");
  return {
    dir, plugin, entry,
    tsPath: fs.realpathSync(path.join(typescript, "index.js")),
    request: {
      projectDir: dir,
      plugins: [config],
      parallelAttestation: {
        workers: 2, batchIndependent: true, typescript: "reviewed",
        plugins: [{ config: JSON.stringify(config), sha256: packageDigest(entry) }],
        dependencies: [],
      },
    },
  };
}

test("attestation mismatch precedes module evaluation and every factory", (t) => {
  const p = project(t);
  fs.writeFileSync(p.entry, "throw new Error('MODULE MUST NOT LOAD ON MISMATCH');\n");
  assert.equal(preflightParallelTransformers({ version: "reviewed" }, p.tsPath, p.request), false);
});

test("matched identity checks metadata without calling a factory", (t) => {
  const p = project(t);
  assert.equal(preflightParallelTransformers({ version: "reviewed" }, p.tsPath, p.request), true);
  assert.equal(preflightParallelTransformers({ version: "other" }, p.tsPath, p.request), false);
  p.request.plugins[0].silent = false;
  assert.equal(preflightParallelTransformers({ version: "reviewed" }, p.tsPath, p.request), false);
});

test("package identity includes package.json and local code, excluding nested node_modules", (t) => {
  const p = project(t);
  const original = packageDigest(p.entry);
  const nested = path.join(p.plugin, "node_modules", "unlisted-registry-dependency");
  fs.mkdirSync(nested, { recursive: true });
  fs.writeFileSync(path.join(nested, "index.js"), "nested dependency");
  assert.equal(packageDigest(p.entry), original);
  fs.writeFileSync(path.join(p.plugin, "package.json"), JSON.stringify({ name: "reviewed-plugin", main: "index.js", version: "changed" }));
  assert.notEqual(packageDigest(p.entry), original);
});

test("owner-listed patched dependency is resolved from its plugin and checked", (t) => {
  const p = project(t);
  const dependency = path.join(p.plugin, "node_modules", "patched-dependency");
  fs.mkdirSync(dependency, { recursive: true });
  fs.writeFileSync(path.join(dependency, "package.json"), JSON.stringify({ name: "patched-dependency", main: "index.js" }));
  const entry = path.join(dependency, "index.js");
  fs.writeFileSync(entry, "module.exports = 'reviewed patch';\n");
  p.request.parallelAttestation.dependencies = [{ from: 0, specifier: "patched-dependency", sha256: packageDigest(entry) }];
  assert.equal(preflightParallelTransformers({ version: "reviewed" }, p.tsPath, p.request), true);
  fs.writeFileSync(entry, "module.exports = 'unreviewed patch';\n");
  assert.equal(preflightParallelTransformers({ version: "reviewed" }, p.tsPath, p.request), false);
});

test("whole ordered options and supported worker count are mandatory", (t) => {
  const p = project(t);
  p.request.parallelAttestation.workers = 3;
  assert.equal(preflightParallelTransformers({ version: "reviewed" }, p.tsPath, p.request), false);
  p.request.parallelAttestation.workers = 2;
  p.request.plugins.push({ transform: "not-attested" });
  assert.equal(preflightParallelTransformers({ version: "reviewed" }, p.tsPath, p.request), false);
});

test("a nested distribution manifest cannot hide the installed package root", (t) => {
  const p = project(t);
  const dist = path.join(p.plugin, "dist", "cjs");
  fs.mkdirSync(dist, { recursive: true });
  fs.writeFileSync(path.join(dist, "package.json"), JSON.stringify({ type: "commonjs" }));
  const entry = path.join(dist, "main.js");
  fs.writeFileSync(entry, "exports.default = () => {};\n");
  const before = packageDigest(entry);
  fs.writeFileSync(path.join(p.plugin, "index.js"), "exports.default = () => 'changed outer code';\n");
  assert.notEqual(packageDigest(entry), before);
});

test("local package walk includes outer code and ignores nested dependencies", (t) => {
  const p = project(t);
  const local = path.join(p.dir, "tools", "local-transformer");
  const dist = path.join(local, "dist", "cjs");
  fs.mkdirSync(dist, { recursive: true });
  fs.writeFileSync(path.join(local, "package.json"), JSON.stringify({ name: "local-transformer" }));
  fs.writeFileSync(path.join(dist, "package.json"), JSON.stringify({ type: "commonjs" }));
  const entry = path.join(dist, "index.js");
  fs.writeFileSync(entry, "module.exports = () => {};\n");
  const before = packageDigest(entry);
  fs.mkdirSync(path.join(local, "node_modules"));
  fs.writeFileSync(path.join(local, "node_modules", "excluded.js"), "excluded");
  assert.equal(packageDigest(entry), before);
  fs.writeFileSync(path.join(local, "outer.js"), "reviewed outer code");
  assert.notEqual(packageDigest(entry), before);
});

test("matched preflight evaluates neither a throwing plugin nor its TS peer", (t) => {
  const p = project(t);
  fs.writeFileSync(p.entry, "throw new Error('ordinary module evaluation only');\n");
  const peer = path.join(p.plugin, "node_modules", "typescript");
  fs.mkdirSync(peer, { recursive: true });
  fs.writeFileSync(path.join(peer, "package.json"), JSON.stringify({ name: "typescript", main: "index.js" }));
  fs.writeFileSync(path.join(peer, "index.js"), "throw new Error('peer must not be evaluated by preflight');\n");
  p.request.parallelAttestation.plugins[0].sha256 = packageDigest(p.entry);
  assert.equal(preflightParallelTransformers({ version: "reviewed" }, p.tsPath, p.request), true);
  assert.equal(require.cache[p.entry], undefined);
  assert.equal(require.cache[path.join(peer, "index.js")], undefined);
});

test("session preflight then ordinary transform preserves evaluation order and response", (t) => {
  const { SidecarServer } = require("../lib/session");
  const p = project(t);
  const enginePath = require.resolve("typescript");
  // A wrapper's metadata version is deliberately different from its engine.
  fs.writeFileSync(path.join(path.dirname(p.tsPath), "package.json"), JSON.stringify({ name: "typescript", version: "0.0.1-wrapper", main: "index.js" }));
  fs.writeFileSync(p.tsPath, `module.exports = require(${JSON.stringify(enginePath)});\n`);
  const ts = require(p.tsPath);
  const key = `rotor-parallel-order:${p.dir}`;
  const events = [];
  globalThis[key] = events;
  t.after(() => { delete globalThis[key]; });
  fs.writeFileSync(p.entry, `globalThis[${JSON.stringify(key)}].push("evaluate"); throw new Error("ordinary top-level failure");\n`);
  fs.writeFileSync(path.join(p.dir, "main.ts"), "export const value = 1;\n");
  const config = path.join(p.dir, "tsconfig.json");
  fs.writeFileSync(config, JSON.stringify({ compilerOptions: { noLib: true }, files: ["main.ts"] }));
  const instrumented = new Proxy(ts, {
    get(target, name) {
      if (name !== "createLanguageService") return target[name];
      return (...args) => {
        const service = target.createLanguageService(...args);
        const getProgram = service.getProgram;
        service.getProgram = (...args) => {
          const program = getProgram.apply(service, args);
          events.push("program");
          return program;
        };
        return service;
      };
    },
  });
  const loader = () => instrumented;
  loader.modulePathFor = () => p.tsPath;
  const server = () => {
    const value = new SidecarServer(loader);
    t.after(() => value.dispose());
    return value;
  };
  const request = { ...p.request, protocol: 1, operation: "transform", tsConfigPath: config,
    compileFileNames: [path.join(p.dir, "main.ts"), path.join(p.dir, "missing.ts")], changedFiles: [] };
  request.parallelAttestation.typescript = ts.version;
  request.parallelAttestation.plugins[0].sha256 = packageDigest(p.entry);
  const semantic = ({ metrics, ...response }) => response;
  const baseline = server().handleRequest(request);
  assert.deepEqual(events, ["program", "evaluate"]);
  events.length = 0;
  const preflighted = server();
  const admission = preflighted.handleRequest({ ...request, operation: "parallel-preflight" });
  assert.equal(admission.parallelReady, true);
  assert.deepEqual(events, [], "preflight must not create a Program or evaluate the throwing module");
  const ordinary = preflighted.handleRequest(request);
  assert.deepEqual(events, ["program", "evaluate"], "one ordinary evaluation after Program creation");
  assert.deepEqual(semantic(ordinary), semantic(baseline));
  events.length = 0;
  const partitioned = server().handleRequest({ ...request, parallelWorker: true });
  assert.equal(partitioned.parallelBatchDiagnostics, 1);
  assert.equal(partitioned.parallelSourceDiagnostics, 1);
  const { parallelBatchDiagnostics, parallelSourceDiagnostics, ...response } = partitioned;
  assert.deepEqual(semantic(response), semantic(baseline));
});
