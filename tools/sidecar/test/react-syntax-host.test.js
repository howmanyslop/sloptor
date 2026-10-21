const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const { PassThrough } = require("node:stream");
const { pathToFileURL } = require("node:url");
const test = require("node:test");
const { counter } = require("./original-react-syntax-fixtures");
const { SidecarProjectSession, SidecarServer, serveStdio } = require("..");

const manifestPath = process.env.SLOPTOR_SYNTAX_PROFILE_MANIFEST;
test("production stdio and API opt-in use the pinned real factory only for authored requests", {
  skip: !manifestPath, timeout: 75000,
}, async (t) => {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  assert.equal(process.env.ANIME_RUSH_APPLIER_CACHE, "off");
  assert.equal(process.env.ANIME_RUSH_APPLIER_TIMINGS, undefined);
  const projectDir = path.join(manifest.outputDir, "production-fixture");
  fs.mkdirSync(path.join(projectDir, "node_modules"), { recursive: true });
  const link = (name, target) => fs.symlinkSync(target, path.join(projectDir, "node_modules", name),
    process.platform === "win32" ? "junction" : "dir");
  link("typescript", path.dirname(path.dirname(manifest.profile.typescript.module.path)));
  link("anime-rush-react-compiler-applier", path.dirname(path.dirname(manifest.profile.entry.path)));
  const fileName = path.join(projectDir, "positive.tsx");
  const tsConfigPath = path.join(projectDir, "tsconfig.json");
  fs.writeFileSync(fileName, counter);
  const writeConfig = (transform = "anime-rush-react-compiler-applier", include = ["**", "!**/*.test.tsx"]) => {
    fs.writeFileSync(tsConfigPath, JSON.stringify({ compilerOptions: {
      target: "ESNext", jsx: "react", module: "Preserve", moduleResolution: "Bundler", moduleDetection: "force",
      noLib: true, types: [], plugins: [{ after: true, transform, include }],
    }, files: ["positive.tsx"] }));
  };
  const counts = { services: 0, programCalls: 0, acquisitions: 0, factories: 0, absentPrograms: 0 };
  const serviceValues = new WeakMap(), registryValues = new WeakMap();
  Object.defineProperty(SidecarProjectSession.prototype, "service", {
    configurable: true,
    get() { return serviceValues.get(this); },
    set(service) {
      if (service) {
        counts.services++;
        const original = service.getProgram;
        service.getProgram = function (...args) { counts.programCalls++; return Reflect.apply(original, this, args); };
      }
      serviceValues.set(this, service);
    },
  });
  Object.defineProperty(SidecarProjectSession.prototype, "documentRegistry", {
    configurable: true,
    get() { return registryValues.get(this); },
    set(registry) {
      for (const key of ["acquireDocument", "acquireDocumentWithKey"]) {
        const original = registry[key];
        registry[key] = function (...args) { counts.acquisitions++; return Reflect.apply(original, this, args); };
      }
      registryValues.set(this, registry);
    },
  });
  const originalLoad = Module._load;
  Module._load = function (name, parent, isMain) {
    const loaded = Reflect.apply(originalLoad, this, [name, parent, isMain]);
    if (name !== manifest.profile.entry.path) return loaded;
    return new Proxy(loaded, { apply(target, receiver, args) {
      counts.factories++;
      if (args[0] === undefined) counts.absentPrograms++;
      return Reflect.apply(target, receiver, args);
    } });
  };
  const servers = new Set();
  const originalHandle = SidecarServer.prototype.handleRequest;
  SidecarServer.prototype.handleRequest = function (request) {
    servers.add(this);
    return originalHandle.call(this, request);
  };
  const oldOptIn = process.env.ROTOR_REACT_SYNTAX_HOST;
  t.after(() => {
    for (const server of servers) server.dispose();
    Module._load = originalLoad;
    SidecarServer.prototype.handleRequest = originalHandle;
    delete SidecarProjectSession.prototype.service;
    delete SidecarProjectSession.prototype.documentRegistry;
    if (oldOptIn === undefined) delete process.env.ROTOR_REACT_SYNTAX_HOST;
    else process.env.ROTOR_REACT_SYNTAX_HOST = oldOptIn;
  });
  const { createBuildSession } = await import(pathToFileURL(path.resolve(__dirname, "../../../api.js")).href);
  const allRecords = [];
  async function run(transport, enabled) {
    process.env.ROTOR_REACT_SYNTAX_HOST = enabled ? "1" : "0";
    writeConfig();
    const records = [];
    let send, close;
    if (transport === "stdio") {
      const input = new PassThrough();
      let resolveResponse;
      const reader = serveStdio({ input, output: { write(text) { resolveResponse(JSON.parse(text)); } } });
      send = request => new Promise(resolve => { resolveResponse = resolve; input.write(`${JSON.stringify(request)}\n`); });
      close = async () => { input.end(); reader.close(); };
    } else {
      const directory = path.join(manifest.outputDir, `api-${enabled}`);
      fs.mkdirSync(directory);
      const session = createBuildSession({ executable: process.execPath,
        executableArgs: [path.join(__dirname, "react-syntax-api-fixture.js"), directory] });
      let sequence = 0;
      send = async request => {
        fs.writeFileSync(path.join(directory, "request.json"), JSON.stringify(request));
        await session.build({ project: projectDir });
        return JSON.parse(fs.readFileSync(path.join(directory, `response-${sequence++}.json`), "utf8"));
      };
      close = () => session.dispose();
    }
    const check = async (name, extra = {}, eligible = true) => {
      const before = { ...counts };
      const request = { protocol: 1, operation: "transform", projectDir, tsConfigPath,
        compileFileNames: [fileName], changedFiles: [], sourceOrigin: "original-authored", ...extra };
      const response = await send(request);
      const delta = Object.fromEntries(Object.keys(counts).map(key => [key, counts[key] - before[key]]));
      const { metrics, ...semantic } = response;
      records.push({ name, response: semantic });
      allRecords.push({ transport, enabled, name, response, counters: delta });
      assert.deepEqual(response.diagnostics, [], name);
      if (request.operation === "validate") {
        assert.equal(delta.factories, 0);
        assert.equal(delta.programCalls, 0);
      } else if (enabled && eligible) {
        assert.equal(delta.services, 0, name);
        assert.equal(delta.programCalls, 0, name);
        assert.equal(delta.acquisitions, 0, name);
        assert.equal(delta.factories, 1, name);
        assert.equal(delta.absentPrograms, 1, name);
      } else {
        assert.equal(delta.programCalls, 1, name);
        assert.equal(delta.factories, 1, name);
        assert.equal(delta.absentPrograms, 0, name);
      }
      return semantic;
    };
    try {
      const first = await check("authored-bare");
      assert.match(first.transformed[0].text, /const memoCache = c\(2\)/);
      await check("missing-origin", { sourceOrigin: undefined }, false);
      await check("unknown-origin", { sourceOrigin: "unknown" }, false);
      await check("suffix-overlay", { sourceOrigin: undefined,
        changedFiles: [{ fileName, text: counter.replace("count: ", "suffix: ") }] }, false);
      assert.deepEqual(await check("next-authored-reverts-suffix", { changedFiles: [{ fileName, text: counter }] }), first);
      writeConfig(manifest.profile.entry.path);
      const absolute = await check("authored-absolute");
      assert.deepEqual(absolute, first);
      writeConfig("anime-rush-react-compiler-applier", ["**"]);
      await check("unreviewed-config", {}, false);
      writeConfig();
      assert.deepEqual(await check("config-restored"), first);
      await check("validation", { operation: "validate", compileFileNames: [] });
    } finally {
      await close();
      fs.writeFileSync(path.join(manifest.outputDir, "production-results.json"), JSON.stringify(allRecords, null, 2));
    }
    return records;
  }
  const baseline = await run("stdio", false);
  assert.deepEqual(await run("stdio", true), baseline);
  assert.deepEqual(await run("api", false), baseline);
  assert.deepEqual(await run("api", true), baseline);
});
