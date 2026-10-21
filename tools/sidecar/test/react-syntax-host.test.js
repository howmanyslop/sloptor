const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const { createHash } = require("node:crypto");
const { PassThrough } = require("node:stream");
const { pathToFileURL } = require("node:url");
const test = require("node:test");
const { counter } = require("./original-react-syntax-fixtures");
const { SidecarProjectSession, SidecarServer, serveStdio } = require("..");
const { BENCH_ORIGINAL_SOURCE, selectOriginalReactInputs } = require("../lib/original-react-syntax");

const manifestPath = process.env.SLOPTOR_SYNTAX_PROFILE_MANIFEST;
test("production stdio and API opt-in use the pinned real factory only for authored requests", {
  skip: !manifestPath, timeout: 75000,
}, async (t) => {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const oxc = manifest.profile.kind === "react-original-source-oxc-v1";
  if (oxc) {
    for (const pin of manifest.artifactFiles) {
      assert.equal(createHash("sha256").update(fs.readFileSync(pin.path)).digest("hex"), pin.sha256, pin.path);
    }
  }
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
  const compileFileNames = [fileName];
  if (oxc) {
    for (const name of ["unchanged.ts", "unchanged.tsx"]) {
      const unchanged = path.join(projectDir, name);
      fs.writeFileSync(unchanged, "export const value = 1;\n");
      compileFileNames.push(unchanged);
    }
  }
  const writeConfig = (transform = "anime-rush-react-compiler-applier", include = ["**", "!**/*.test.tsx"], extraOptions = {}) => {
    fs.writeFileSync(tsConfigPath, JSON.stringify({ compilerOptions: {
      target: "ESNext", jsx: "react", module: "Preserve", moduleResolution: "Bundler", moduleDetection: "force",
      noLib: true, types: [], ...extraOptions, plugins: [{ after: true, transform, include, ...(oxc ? { backend: "oxc" } : {}) }],
    }, files: compileFileNames.map(name => path.basename(name)) }));
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
  const factoryConfigs = [];
  Module._load = function (name, parent, isMain) {
    const loaded = Reflect.apply(originalLoad, this, [name, parent, isMain]);
    if (name !== manifest.profile.entry.path) return loaded;
    return new Proxy(loaded, { apply(target, receiver, args) {
      counts.factories++;
      if (args[0] === undefined) counts.absentPrograms++;
      if (oxc) {
        assert.equal(args[1].backend, "oxc", "the real factory receives the unchanged backend option");
        factoryConfigs.push(JSON.parse(JSON.stringify(args[1])));
      }
      return Reflect.apply(target, receiver, args);
    } });
  };
  const servers = new Set();
  let latestServer;
  const originalHandle = SidecarServer.prototype.handleRequest;
  SidecarServer.prototype.handleRequest = function (request) {
    servers.add(this);
    latestServer = this;
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
  function checkOxcProfileGates() {
    const session = [...latestServer.sessions.values()][0];
    const profile = session.reactSyntaxProfile;
    assert.equal(profile.kind, "react-original-source-oxc-v1");
    const freeze = value => {
      if (value && typeof value === "object") { Object.values(value).forEach(freeze); Object.freeze(value); }
      return value;
    };
    const parseHost = { useCaseSensitiveFileNames: session.ts.sys.useCaseSensitiveFileNames,
      readDirectory: session.ts.sys.readDirectory, fileExists: name => session.fileExists(name),
      readFile: name => session.readFile(name), getCurrentDirectory: () => projectDir,
      onUnRecoverableConfigFileDiagnostic: () => undefined };
    const chain = session.parsed.options.plugins;
    const select = (candidate = profile, configs = chain, configured = configs) => {
      const view = Object.create(session);
      view.parsed = { ...session.parsed, options: { ...session.parsed.options, plugins: configured } };
      return selectOriginalReactInputs(view, { projectDir, tsConfigPath, compileFileNames,
        [BENCH_ORIGINAL_SOURCE]: candidate }, configs, parseHost, candidate);
    };
    const before = { ...counts };
    assert(select());
    const pins = ["entry", "chunk", "runtime", "binding", "upstream"].map(key => [key, profile[key]]);
    pins.push(...profile.imports.map((pin, index) => [index, pin]));
    for (const [key, pin] of pins) {
      const invalid = typeof key === "number"
        ? { ...profile, imports: profile.imports.map((value, index) => index === key ? { ...value, sha256: "0".repeat(64) } : value) }
        : { ...profile, [key]: { ...pin, sha256: "0".repeat(64) } };
      assert.equal(select(freeze(invalid)), undefined, String(key));
      // Simulate a changed artifact read only during admission. Do not rewrite
      // an installed bundle/binding or substitute the factory used for parity.
      const readFile = fs.readFileSync;
      try {
        fs.readFileSync = function (name, ...args) {
          const contents = Reflect.apply(readFile, this, [name, ...args]);
          return name === pin.path ? Buffer.concat([contents, Buffer.from("drift")]) : contents;
        };
        assert.equal(select(), undefined, `${key} artifact digest`);
      } finally {
        fs.readFileSync = readFile;
      }
    }
    assert.equal(select(freeze({ ...profile, entry: profile.chunk })), undefined, "wrong entry");
    assert.equal(select(freeze({ ...profile, runtime: { ...profile.runtime,
      path: path.join(projectDir, "missing.cjs") } })), undefined, "missing runtime");
    const { backend, ...withoutBackend } = chain[0];
    for (const config of [withoutBackend, { ...chain[0], backend: "babel" }, { ...chain[0], backend: "rust" },
      { ...chain[0], refresh: {} }, { ...chain[0], include: ["**"] }, { ...chain[0], type: "program" }]) {
      // Even an in-process profile cannot attest an unreviewed option shape.
      const invalid = freeze(JSON.parse(JSON.stringify({ ...profile, chains: [[config]] })));
      assert.equal(select(invalid, [config]), undefined, JSON.stringify(config));
    }
    const mixed = [{ transform: "jecs" }, ...chain, { transform: "editor-name" }];
    assert.equal(select(profile, mixed), undefined, "mixed chain");
    assert.equal(select(profile, chain, mixed), undefined, "request cannot conceal configured plugins");
    assert.deepEqual(counts, before, "profile rejection performs no factory, service or Program work");
  }
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
    const check = async (name, extra = {}, eligible = true, diagnostic = false) => {
      const before = { ...counts };
      const configStart = factoryConfigs.length;
      const request = { protocol: 1, operation: "transform", projectDir, tsConfigPath,
        compileFileNames, changedFiles: [], sourceOrigin: "original-authored", ...extra };
      const response = await send(request);
      const delta = Object.fromEntries(Object.keys(counts).map(key => [key, counts[key] - before[key]]));
      const { metrics, ...semantic } = response;
      const configs = oxc ? { factoryConfigs: factoryConfigs.slice(configStart) } : {};
      records.push({ name, response: semantic, ...configs });
      allRecords.push({ transport, enabled, name, response, counters: delta, ...configs });
      if (diagnostic) assert(response.diagnostics.length > 0, name);
      else assert.deepEqual(response.diagnostics, [], name);
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
      assert.match(first.transformed[0].text, oxc ? /memoCache/ : /const memoCache = c\(2\)/);
      if (oxc) {
        assert.equal(first.transformed.length, 1, "ordinary TS/TSX files stay unchanged");
        const trace = JSON.parse(first.transformed[0].traceMap);
        assert(trace.sources.some(name => name.endsWith("positive.tsx")));
        assert(trace.mappings.length > 0);
        if (transport === "stdio" && enabled) checkOxcProfileGates();
        writeConfig("anime-rush-react-compiler-applier", ["**", "!**/use-sync-external-store-with-selector.tsx", "!**/*.test.tsx"]);
        assert.deepEqual(await check("authored-hooks-options"), first);
        writeConfig(undefined, undefined, { hostSyntaxUnknownOption: true });
        await check("config-diagnostic", {}, true, true);
        writeConfig();
      }
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
