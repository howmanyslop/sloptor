// Run only by original-react-syntax.test.js in two bounded, cache-off children.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");
const { createHash } = require("node:crypto");
const { counter, sources } = require("./original-react-syntax-fixtures");
const { BENCH_ORIGINAL_SOURCE } = require("../lib/original-react-syntax");

function freeze(value) {
  if (value && typeof value === "object") {
    Object.values(value).forEach(freeze);
    Object.freeze(value);
  }
  return value;
}

function sha256(fileName) {
  return createHash("sha256").update(fs.readFileSync(fileName)).digest("hex");
}

function main() {
  const manifest = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
  const arm = process.argv[3];
  assert(["baseline", "candidate"].includes(arm));
  assert.equal(process.env.ANIME_RUSH_APPLIER_CACHE, "off");
  assert.equal(process.env.ANIME_RUSH_APPLIER_TIMINGS, undefined);
  assert.equal(sha256(process.execPath), manifest.node.sha256);
  for (const pin of manifest.artifactFiles) assert.equal(sha256(pin.path), pin.sha256, pin.path);
  const profile = freeze(manifest.profile);
  const ts = require(profile.typescript.module.path);
  assert.equal(ts.version, "6.0.3");
  assert.equal(ts, require(profile.typescript.implementation.path));
  const { SidecarProjectSession, SidecarServer } = require(arm === "baseline"
    ? manifest.baselineSessionPath : "../lib/session");
  const fixtureDir = path.resolve(manifest.outputDir, "fixture");
  fs.mkdirSync(path.join(fixtureDir, "src"), { recursive: true });
  const file = (name) => path.join(fixtureDir, "src", name);
  const configPath = path.join(fixtureDir, "tsconfig.json");
  for (const [name, text] of Object.entries(sources)) fs.writeFileSync(file(name), text);
  fs.writeFileSync(path.join(fixtureDir, "imported.ts"), "export const imported = 1;\n");
  const baseChain = profile.chains[0];
  const hookChain = profile.chains[1];
  const compilerOptions = {
    target: "ESNext", jsx: "react", module: "Preserve", moduleResolution: "Bundler",
    moduleDetection: "force", noLib: true, types: [], plugins: baseChain,
  };
  const writeConfig = (extra = {}, options = {}) => fs.writeFileSync(configPath, JSON.stringify({
    compilerOptions: { ...compilerOptions, ...options }, include: ["src/**/*.ts", "src/**/*.tsx"], ...extra,
  }));
  writeConfig();

  const counters = { services: 0, programCalls: 0, registryAcquires: 0, factories: 0, absentPrograms: 0 };
  const services = new WeakMap();
  const registries = new WeakMap();
  const disposals = new WeakSet();
  // Test-only observation of real service assignments; neither TS nor React is
  // replaced. No service/getProgram/registry activity means this host cannot
  // reach LanguageService's internal createProgram + getTypeChecker sequence.
  Object.defineProperty(SidecarProjectSession.prototype, "service", {
    configurable: true,
    get() { return services.get(this); },
    set(service) {
      if (service && !disposals.has(service)) {
        counters.services += 1;
        disposals.add(service);
        const getProgram = service.getProgram;
        service.getProgram = function (...args) {
          counters.programCalls += 1;
          return getProgram.apply(this, args);
        };
      }
      services.set(this, service);
    },
  });
  Object.defineProperty(SidecarProjectSession.prototype, "documentRegistry", {
    configurable: true,
    get() { return registries.get(this); },
    set(registry) {
      for (const key of ["acquireDocument", "acquireDocumentWithKey"]) {
        const acquire = registry[key];
        registry[key] = function (...args) {
          counters.registryAcquires += 1;
          return acquire.apply(this, args);
        };
      }
      registries.set(this, registry);
    },
  });

  let step = "";
  let programCallsBeforeRequest = 0;
  let positiveChanges = 0;
  const factoryInputs = [];
  const syntheticEvents = [];
  global.__syntaxProfileEvent = (kind, hasProgram) => {
    if (kind === "evaluate") assert(counters.programCalls > programCallsBeforeRequest,
      "unsupported plugin evaluated before the ordinary getProgram path");
    if (hasProgram !== undefined) assert.equal(hasProgram, true, kind);
    syntheticEvents.push({ step, kind });
  };
  const originalLoad = Module._load;
  const observedFactories = new WeakMap();
  Module._load = function (request, parent, isMain) {
    const loaded = originalLoad.call(this, request, parent, isMain);
    // Transparent test observer around the actual unchanged pinned factory.
    // Reflect.apply always invokes that export; no replacement transform logic.
    if (request !== profile.entry.path) return loaded;
    if (!observedFactories.has(loaded)) {
      observedFactories.set(loaded, new Proxy(loaded, {
        apply(target, thisArg, args) {
          counters.factories += 1;
          if (args[0] === undefined) counters.absentPrograms += 1;
          const contextFactory = Reflect.apply(target, thisArg, args);
          const input = { step, config: args[1], cwd: process.cwd(), argv: process.argv.slice(2), files: [] };
          factoryInputs.push(input);
          return (context) => {
            const visit = contextFactory(context);
            return (source) => {
              assert.equal(ts.getOriginalNode(source), source);
              for (const child of source.statements) assert.equal(child.parent, source);
              input.files.push({
                fileName: source.fileName, text: source.text, languageVersion: source.languageVersion,
                scriptKind: source.scriptKind, jsDocParsingMode: source.jsDocParsingMode,
                impliedNodeFormat: source.impliedNodeFormat, isDeclarationFile: source.isDeclarationFile,
                externalModule: !!source.externalModuleIndicator,
              });
              const result = visit(source);
              if (result !== source && /memoCache/.test(result.text)) positiveChanges += 1;
              return result;
            };
          };
        },
      }));
    }
    return observedFactories.get(loaded);
  };

  // Same process inputs in both children, restored by real SidecarServer.
  process.argv.splice(2, process.argv.length - 2, "--project", "unresolved-config.json");
  const loader = () => ts;
  loader.modulePathFor = () => profile.typescript.module.path;
  const server = new SidecarServer(loader, arm === "candidate" ? { benchOriginalReactProfile: profile } : undefined);
  const records = [];
  let completed = false;
  let syntaxRequests = 0;
  let genericRequests = 0;
  const send = (name, names, { eligible = true, origin = "authored", requestProfile = profile, changes = [], ...extra } = {}) => {
    step = name;
    const before = { ...counters };
    programCallsBeforeRequest = counters.programCalls;
    const cwd = process.cwd();
    const argv = [...process.argv];
    const request = {
      protocol: 1, operation: "transform", projectDir: fixtureDir, tsConfigPath: configPath,
      compileFileNames: names.map((name) => path.isAbsolute(name) ? name : file(name)), changedFiles: changes,
      ...extra,
    };
    if (origin !== "missing") request[BENCH_ORIGINAL_SOURCE] = origin === "authored" ? requestProfile : origin;
    const response = server.handleRequest(request);
    assert.equal(process.cwd(), cwd);
    assert.deepEqual(process.argv, argv);
    const delta = Object.fromEntries(Object.keys(before).map((key) => [key, counters[key] - before[key]]));
    const { metrics, ...semantic } = response;
    records.push({ name, response: semantic, counters: delta });
    if (request.operation === "validate") {
      assert.equal(delta.factories, 0);
      assert.equal(delta.programCalls, 0);
    } else if (arm === "candidate" && eligible) {
      syntaxRequests += 1;
      assert.equal(delta.services, 0, name);
      assert.equal(delta.programCalls, 0, name);
      assert.equal(delta.registryAcquires, 0, name);
      assert.equal(delta.factories, 1, name);
      assert.equal(delta.absentPrograms, 1, name);
      assert.equal([...server.sessions.values()][0].service, undefined);
    } else {
      genericRequests += 1;
      assert.equal(delta.programCalls, 1, name);
      assert.equal(delta.absentPrograms, 0, name);
    }
    return semantic;
  };

  try {
    const first = send("positive-narrow", ["positive.tsx"], { rootFileNames: [file("positive.tsx")] });
    assert.equal(first.diagnostics.length, 0);
    assert.equal(first.transformed.length, 1);
    assert.match(first.transformed[0].text, /memoCache/);
    assert.match(first.transformed[0].text, /const memoCache = c\(2\)/);
    const firstTrace = JSON.parse(first.transformed[0].traceMap);
    assert(firstTrace.sources.some((name) => name.endsWith("positive.tsx")));
    assert(firstTrace.mappings.length > 0);
    send("unchanged-repeat", ["positive.tsx"], { rootFileNames: [file("positive.tsx")] });
    send("configured-but-not-effective-root", ["unchanged.ts"], {
      eligible: false, rootFileNames: [file("positive.tsx")],
    });
    const batch = ["unchanged.ts", "unchanged-tsx.tsx", "excluded.test.tsx", "directive.ts", "comment.ts", "crlf.tsx", "collision.tsx"];
    const widened = send("root-widening", batch, { rootFileNames: batch.map(file) });
    assert.equal(widened.diagnostics.length, 0);
    for (const unchanged of ["unchanged.ts", "unchanged-tsx.tsx", "excluded.test.tsx", "comment.ts"]) {
      assert(!widened.transformed.some((item) => item.fileName.endsWith(`/${unchanged}`)), unchanged);
    }
    for (const changed of ["directive.ts", "crlf.tsx", "collision.tsx"]) {
      assert(widened.transformed.some((item) => item.fileName.endsWith(`/${changed}`)), changed);
    }
    send("unnarrowed", ["positive.tsx", "positive.tsx"]);
    send("narrow-after-unnarrowed", ["crlf.tsx"], { rootFileNames: [file("positive.tsx")] });
    const changed = counter.replace('"count: "', '"changed: "');
    const overlay = send("authored-overlay", ["positive.tsx"], { changes: [{ fileName: file("positive.tsx"), text: changed }] });
    assert.match(overlay.transformed[0].text, /changed:/);
    send("overlay-repeat", ["positive.tsx"]);
    // Protocol 1 has no overlay-removal request. Exercise the existing session
    // state transition explicitly in the harness, without adding native API.
    for (const session of server.sessions.values()) {
      const canonical = session.canonicalize(file("positive.tsx"));
      assert(session.overrides.delete(canonical));
      session.versions.set(canonical, session.getScriptVersion(file("positive.tsx")) + 1);
      session.projectVersion += 1;
    }
    assert.deepEqual(send("overlay-removal-test-hook", ["positive.tsx"]), first);
    send("before-disk-removal", ["remove.ts"]);
    fs.unlinkSync(file("remove.ts"));
    send("disk-removal", ["remove.ts"], { eligible: false });
    send("missing", ["missing.ts"], { eligible: false });
    send("declaration", ["declaration.d.ts"], { eligible: false });
    const babel = send("realBabelError", ["realBabelError.tsx"]);
    assert.equal(babel.diagnostics.length, 1);
    assert.equal(babel.diagnostics[0].code, "sidecar-internal");
    assert.match(babel.diagnostics[0].message, /SyntaxError|Unexpected|JSX/);
    writeConfig({}, { plugins: hookChain });
    assert.equal(send("excluded-hook", ["use-sync-external-store-with-selector.tsx"]).transformed.length, 0);
    writeConfig({ include: ["src/positive.tsx"] });
    send("config-include-change", ["positive.tsx"]);
    send("removed-config-root", ["unchanged.ts"], { eligible: false });
    writeConfig();
    send("config-root-widened", ["unchanged.ts"]);
    writeConfig({}, { hostSyntaxUnknownOption: true });
    assert(send("config-diagnostic", ["positive.tsx"]).diagnostics.length > 0);
    writeConfig();
    send("post-transform-origin", ["positive.tsx"], { eligible: false, origin: "post-transform" });
    send("missing-origin", ["positive.tsx"], { eligible: false, origin: "missing" });
    send("back-to-syntax", ["positive.tsx"]);
    const invalidProfiles = [
      ["mutable-profile", { ...profile }],
      ["entry-identity", freeze({ ...profile, entry: { ...profile.entry, sha256: "0".repeat(64) } })],
      ["chunk-identity", freeze({ ...profile, chunk: { ...profile.chunk, sha256: "0".repeat(64) } })],
      ["missing-chunk", freeze({ ...profile, chunk: { ...profile.chunk, path: path.join(fixtureDir, "missing.cjs") } })],
      ["typescript-identity", freeze({ ...profile, typescript: { ...profile.typescript,
        implementation: { ...profile.typescript.implementation, sha256: "0".repeat(64) } } })],
    ];
    for (const [name, invalid] of invalidProfiles) {
      if (arm === "candidate") [...server.sessions.values()][0].benchOriginalReactProfile = invalid;
      send(name, ["positive.tsx"], { eligible: false, requestProfile: invalid });
    }
    if (arm === "candidate") [...server.sessions.values()][0].benchOriginalReactProfile = profile;
    send("profile-restored", ["positive.tsx"]);

    const pluginsDir = path.join(fixtureDir, "plugins");
    fs.mkdirSync(pluginsDir, { recursive: true });
    const synthetic = (name, body) => {
      const pluginPath = path.join(pluginsDir, `${name}.cjs`);
      fs.writeFileSync(pluginPath, 'global.__syntaxProfileEvent("evaluate");\n' + body);
      return { after: true, transform: pluginPath };
    };
    const checker = synthetic("checker", 'module.exports = (program) => { global.__syntaxProfileEvent("checker", !!program.getTypeChecker()); return () => source => source; };');
    const hook = synthetic("hook", 'module.exports = () => () => source => source; module.exports.shouldTransformSourceFile = (source, program) => { global.__syntaxProfileEvent("hook", !!program); return true; };');
    const raw = { ...synthetic("raw", 'module.exports = (context, program) => { global.__syntaxProfileEvent("raw", !!program); return source => source; };'), type: "raw" };
    const custom = { ...synthetic("custom", 'exports.named = program => { global.__syntaxProfileEvent("custom", !!program); return () => source => source; };'), import: "named" };
    const second = synthetic("second", 'module.exports = program => { global.__syntaxProfileEvent("second", !!program); return () => source => source; };');
    for (const [name, chain] of [["checker-consumer", [checker]], ["hook", [hook]], ["raw", [raw]], ["custom-import", [custom]], ["second-plugin", [...baseChain, second]]]) {
      writeConfig({}, { plugins: chain });
      send(name, ["positive.tsx"], { eligible: false });
    }
    send("request-cannot-hide-configured-second-plugin", ["positive.tsx"], { eligible: false, plugins: baseChain });
    writeConfig();
    send("generic-to-syntax", ["positive.tsx"]);
    send("request-chain-mismatch", ["positive.tsx"], { eligible: false, plugins: [checker] });
    writeConfig({}, { plugins: [...baseChain, {}] });
    send("filtered-invalid-config-entry", ["positive.tsx"], { eligible: false });
    writeConfig({ files: ["src/importer.ts"], include: [] });
    send("import-only-nonroot", [path.join(fixtureDir, "imported.ts")], { eligible: false, rootFileNames: [file("importer.ts")] });
    send("nonroot-request-cannot-admit", [path.join(fixtureDir, "imported.ts")], { eligible: false, rootFileNames: [path.join(fixtureDir, "imported.ts")] });
    writeConfig();
    const alias = path.join(fixtureDir, "alias");
    if (!fs.existsSync(alias)) fs.symlinkSync(path.join(fixtureDir, "src"), alias, process.platform === "win32" ? "junction" : "dir");
    send("alias", [path.join(alias, "positive.tsx")], { eligible: false });
    const referenceDir = path.join(fixtureDir, "reference");
    fs.mkdirSync(path.join(referenceDir, "out"), { recursive: true });
    fs.writeFileSync(path.join(referenceDir, "ref.ts"), "export const ref = 1;\n");
    fs.writeFileSync(path.join(referenceDir, "out", "ref.d.ts"), "export declare const ref: number;\n");
    fs.writeFileSync(path.join(referenceDir, "tsconfig.json"), JSON.stringify({
      compilerOptions: { composite: true, outDir: "out", rootDir: "." }, files: ["ref.ts"],
    }));
    writeConfig({ references: [{ path: "./reference" }] });
    const ownRootWithReference = send("own-root-with-reference", ["positive.tsx"]);
    assert.deepEqual(ownRootWithReference, first);
    send("project-reference-redirect", [path.join(referenceDir, "out", "ref.d.ts")], { eligible: false });
    writeConfig({}, { target: "ES2020" });
    send("unsupported-parser-options", ["positive.tsx"], { eligible: false });
    writeConfig({}, { plugins: [{ transform: second.transform, afterDeclarations: true }] });
    const validation = send("declaration-only-validate", [], { operation: "validate" });
    assert.equal(validation.afterDeclarationsTransformers, 1);
    assert.equal(validation.transformed.length, 0);
    writeConfig();
    send("after-validate-syntax", ["positive.tsx"]);
    assert(positiveChanges > 0);
    completed = true;
  } finally {
    fs.writeFileSync(path.join(manifest.outputDir, `${arm}.json`), JSON.stringify({
      completed, records, factoryInputs, syntheticEvents, syntaxRequests, genericRequests, positiveChanges,
      environment: { node: process.version, compileCache: process.env.NODE_COMPILE_CACHE ?? null,
        disableCompileCache: process.env.NODE_DISABLE_COMPILE_CACHE ?? null },
    }, null, 2));
    server.dispose();
    Module._load = originalLoad;
    delete global.__syntaxProfileEvent;
  }
}

main();
