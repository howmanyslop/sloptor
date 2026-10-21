// Source-reviewed React factories, original authored input only. Profiles come
// from the opt-in host resolver or the private finite-test harness.
const fs = require("node:fs");
const path = require("node:path");
const { createHash } = require("node:crypto");
const { isDeepStrictEqual } = require("node:util");

// Setting this symbol to the *same* profile object attests that every selected
// input and accumulated overlay is authored text, with no preceding transform.
// JSON/stdin callers cannot supply this in-process provenance assertion.
const BENCH_ORIGINAL_SOURCE = Symbol("bench original authored React input");

function frozenData(value) {
  if (value === null || typeof value !== "object") {
    return ["string", "number", "boolean", "undefined"].includes(typeof value) || value === null;
  }
  if (!Object.isFrozen(value) || (!Array.isArray(value) && Object.getPrototypeOf(value) !== Object.prototype)) {
    return false;
  }
  return Reflect.ownKeys(value).every((key) => {
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    return typeof key === "string" && "value" in descriptor && frozenData(descriptor.value);
  });
}

function realpath(fileName) {
  return fs.realpathSync.native?.(fileName) ?? fs.realpathSync(fileName);
}

function matchesPin(pin) {
  return pin && path.isAbsolute(pin.path) && realpath(pin.path) === pin.path &&
    /^[a-f0-9]{64}$/.test(pin.sha256) &&
    createHash("sha256").update(fs.readFileSync(pin.path)).digest("hex") === pin.sha256;
}

function supportedChain(configs, kind) {
  if (!Array.isArray(configs) || configs.length !== 1) return false;
  const config = configs[0];
  const oxc = kind === "react-original-source-oxc-v1";
  return config && config.after === true && typeof config.transform === "string" &&
    (config.type === undefined || config.type === "program") &&
    (!oxc || config.backend === "oxc") &&
    Object.keys(config).every((key) => ["after", "transform", "type", "include", ...(oxc ? ["backend"] : [])].includes(key)) &&
    Array.isArray(config.include) && config.include.every((glob) => typeof glob === "string");
}

function directLocalPath(session, fileName) {
  if (!path.isAbsolute(fileName)) return false;
  const absolute = path.normalize(fileName);
  const relative = path.relative(session.projectDir, absolute);
  return relative !== "" && relative !== ".." && !relative.startsWith(`..${path.sep}`) &&
    !path.isAbsolute(relative) && !absolute.split(path.sep).some((part) => part.toLowerCase() === "node_modules") &&
    realpath(absolute) === absolute;
}

function disjointProjectReferences(session, inputs, parseHost) {
  const selected = inputs.map(({ fileName }) => session.canonicalize(fileName));
  const visiting = new Set([session.canonicalize(session.tsConfigPath)]);
  const visited = new Set();
  const within = (directory, file) => {
    const relative = path.relative(session.canonicalize(directory), file);
    return relative === "" || (!path.isAbsolute(relative) && relative !== ".." && !relative.startsWith(`..${path.sep}`));
  };
  const visit = (reference) => {
    const configPath = session.ts.resolveProjectReferencePath(reference);
    const canonical = session.canonicalize(configPath);
    if (visiting.has(canonical) || realpath(configPath) !== path.normalize(configPath)) return false;
    if (visited.has(canonical)) return true;
    visiting.add(canonical);
    const parsed = session.ts.getParsedCommandLineOfConfigFile(configPath, {}, parseHost);
    if (!parsed || parsed.errors.length !== 0) return false;
    // This is the input set TS uses for source -> declaration redirects, even
    // when a source was also explicitly configured as a root of this project.
    if (parsed.fileNames.some((name) => selected.includes(session.canonicalize(name)))) return false;
    // Keep this first extension conservative about overlapping output ownership.
    // Declaration files themselves are already excluded by the selected-root gate.
    if ([parsed.options.outDir, parsed.options.declarationDir].some((directory) =>
      directory && selected.some((file) => within(directory, file)))) return false;
    if (parsed.options.outFile) return false;
    if (!(parsed.projectReferences ?? []).every(visit)) return false;
    visiting.delete(canonical);
    visited.add(canonical);
    return true;
  };
  return (session.parsed.projectReferences ?? []).every(visit);
}

function selectOriginalReactInputs(session, request, configs, parseHost, profile = session.benchOriginalReactProfile) {
  try {
    if (!profile || request[BENCH_ORIGINAL_SOURCE] !== profile || !frozenData(profile) ||
        !["react-original-source-v1", "react-original-source-oxc-v1"].includes(profile.kind) ||
        !supportedChain(configs, profile.kind) ||
        !Array.isArray(profile.chains) || !profile.chains.some((chain) =>
          supportedChain(chain, profile.kind) && isDeepStrictEqual(configs, chain))) return;
    const oxc = profile.kind === "react-original-source-oxc-v1";
    // Also bind the native path selected by the actual bundle's workspace
    // lookup. A private profile cannot widen this release's options or pins.
    if (oxc && !isDeepStrictEqual(profile, require("./react-syntax-profile").resolveReactSyntaxProfile(
      session.projectDir, session.tsModulePath, configs,
    ))) return;
    if (realpath(request.projectDir) !== path.normalize(request.projectDir) ||
        realpath(request.tsConfigPath) !== path.normalize(request.tsConfigPath)) return;

    // Check the raw resolved chain too: getPluginConfigs filters malformed
    // entries, and an explicit request must not hide another configured plugin.
    if (!isDeepStrictEqual(session.parsed.options.plugins, configs)) return;
    const { ts } = session;
    const options = session.parsed.options;
    if (ts.version !== profile.typescript.version || ts.version !== "6.0.3" ||
        options.target !== ts.ScriptTarget.ESNext || options.jsx !== ts.JsxEmit.React ||
        options.module !== ts.ModuleKind.Preserve || options.moduleResolution !== ts.ModuleResolutionKind.Bundler ||
        options.moduleDetection !== ts.ModuleDetectionKind.Force || options.allowJs ||
        options.allowNonTsExtensions || options.noResolve || options.preserveSymlinks) return;

    const { entry, chunk, typescript } = profile;
    const bundles = [entry, chunk, ...(oxc ? [profile.runtime] : [])];
    if (![...bundles, ...(oxc ? [profile.binding, profile.upstream, ...profile.imports] : []), typescript.module, typescript.implementation].every(matchesPin) ||
        session.tsModulePath !== typescript.module.path ||
        require.cache[typescript.module.path]?.exports !== ts ||
        require.cache[typescript.implementation.path]?.exports !== ts ||
        realpath(require.resolve(configs[0].transform, { paths: [session.projectDir] })) !== entry.path ||
        realpath(require.resolve(`./${path.basename(chunk.path)}`, { paths: [path.dirname(entry.path)] })) !== chunk.path ||
        bundles.some((pin) =>
          realpath(require.resolve("typescript", { paths: [path.dirname(pin.path)] })) !== typescript.module.path)) return;
    // The existing TS6 wrapper is permitted only with its separately pinned
    // actual implementation and the unchanged forwarding source.
    if (typescript.module.path !== typescript.implementation.path &&
        (fs.readFileSync(typescript.module.path, "utf8").trim() !== 'module.exports = require("@typescript/old");' ||
         realpath(require.resolve("@typescript/old", { paths: [path.dirname(typescript.module.path)] })) !== typescript.implementation.path)) return;

    const configured = new Map(session.parsed.fileNames.map((name) => [session.canonicalize(name), name]));
    const effective = new Set(session.getScriptFileNames().map((name) => session.canonicalize(name)));
    if (request.compileFileNames.length === 0) return;
    const inputs = [];
    for (const name of request.compileFileNames) {
      const canonical = session.canonicalize(name);
      const configuredName = configured.get(canonical);
      // setRootLimit/overlays may remember arbitrary paths on the generic path.
      // Admission requires configured AND effective membership, never adds it.
      if (!configuredName || !effective.has(canonical) || !directLocalPath(session, name) ||
          !directLocalPath(session, configuredName) || !/\.tsx?$/.test(name) || /\.d\.ts$/.test(name)) return;
      const text = session.readFile(name);
      if (typeof text !== "string") return;
      inputs.push({ fileName: ts.normalizePath(session.actualPaths.get(canonical)), text });
    }
    if (!disjointProjectReferences(session, inputs, parseHost)) return;
    return { entryPath: entry.path, inputs };
  } catch {
    // Missing files, resolution failures, malformed profiles and aliases are
    // ordinary requests. No plugin has been required or factory invoked here.
    return undefined;
  }
}

function parseOriginalReactInputs(session, inputs) {
  const { ts } = session;
  const options = session.parsed.options;
  // Per request only. Preserve duplicate identity like Program.getSourceFile.
  const sources = new Map();
  return inputs.map(({ fileName, text }) => {
    const canonical = session.canonicalize(fileName);
    if (!sources.has(canonical)) {
      const source = ts.createLanguageServiceSourceFile(
        fileName,
        ts.ScriptSnapshot.fromString(text),
        {
          languageVersion: ts.getEmitScriptTarget(options),
          impliedNodeFormat: ts.getImpliedNodeFormatForFile(fileName, undefined, ts.sys, options),
          setExternalModuleIndicator: ts.getSetExternalModuleIndicator(options),
          // createDocumentRegistry in this host receives no JSDoc override.
          jsDocParsingMode: undefined,
        },
        String(session.getScriptVersion(fileName)),
        true,
        fileName.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
      );
      if (ts.getOriginalNode(source) !== source || source.isDeclarationFile) {
        throw new Error("Original React syntax source assertion failed");
      }
      sources.set(canonical, source);
    }
    return sources.get(canonical);
  });
}

module.exports = { BENCH_ORIGINAL_SOURCE, selectOriginalReactInputs, parseOriginalReactInputs };
