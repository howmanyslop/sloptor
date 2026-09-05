const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const {
  createInternalDiagnostic,
  createProtocolDiagnostic,
  createRequestDiagnostic,
  toProtocolDiagnostic,
} = require("./diagnostics");
const {
  collectRequireClosure,
  createTransformerList,
  flattenIntoTransformers,
  getPluginConfigs,
  pluginMetrics,
  resetPluginMetrics,
  validatePluginConfigs,
  wrapTransformersWithParentFix,
} = require("./plugins");

function createParseHost(session, ts) {
  return {
    useCaseSensitiveFileNames: ts.sys.useCaseSensitiveFileNames,
    readDirectory: ts.sys.readDirectory,
    fileExists: (fileName) => session.configFileExists(fileName),
    readFile: (fileName) => session.readConfigFile(fileName),
    getCurrentDirectory: () => session.projectDir,
    onUnRecoverableConfigFileDiagnostic: () => undefined,
  };
}

function createServiceHost(session, ts) {
  return {
    getCompilationSettings: () => session.parsed.options,
    getCurrentDirectory: () => session.projectDir,
    getDefaultLibFileName: (options) => ts.getDefaultLibFilePath(options),
    getDirectories: ts.sys.getDirectories,
    directoryExists: ts.sys.directoryExists,
    fileExists: (fileName) => session.fileExists(fileName),
    getProjectVersion: () => String(session.projectVersion),
    getScriptFileNames: () => session.getScriptFileNames(),
    getScriptSnapshot: (fileName) => {
      const text = session.readFile(fileName);
      return text === undefined ? undefined : ts.ScriptSnapshot.fromString(text);
    },
    getScriptVersion: (fileName) => String(session.getScriptVersion(fileName)),
    readDirectory: ts.sys.readDirectory,
    readFile: (fileName) => session.readFile(fileName),
    realpath: ts.sys.realpath ? (fileName) => ts.sys.realpath(fileName) : undefined,
    useCaseSensitiveFileNames: () => ts.sys.useCaseSensitiveFileNames,
  };
}

function normalizePath(fileName) {
  const resolved = path.normalize(path.resolve(fileName));
  try {
    return fs.realpathSync.native?.(resolved) ?? fs.realpathSync(resolved);
  } catch {
    return resolved;
  }
}

// Transformer packages read both cwd and `--project` directly. Keep those
// process-level inputs aligned with the physical request paths before loading
// a plugin: otherwise an alias such as /var or a Windows short-name path can
// be compared with TypeScript's canonical source and output paths.
function syncProjectProcessPaths(projectDir, tsConfigPath) {
  process.chdir(projectDir);
  for (let index = 0; index < process.argv.length - 1; index += 1) {
    if (process.argv[index] === "-p" || process.argv[index] === "--project") {
      process.argv[index + 1] = tsConfigPath;
    }
  }
}

function fileDigest(fileName) {
  try {
    return crypto.createHash("sha256").update(fs.readFileSync(fileName)).digest("hex");
  } catch (error) {
    if (error?.code === "ENOENT") {
      return undefined;
    }
    throw error;
  }
}

function fileStamp(fileName) {
  try {
    const stat = fs.statSync(fileName, { bigint: true });
    return `${stat.dev}:${stat.ino}:${stat.size}:${stat.mtimeNs}:${stat.ctimeNs}`;
  } catch (error) {
    if (error?.code === "ENOENT") {
      return undefined;
    }
    throw error;
  }
}

class SidecarProjectSession {
  constructor(ts, projectDir, tsConfigPath, tsModulePath) {
    this.ts = ts;
    this.projectDir = normalizePath(projectDir);
    this.tsConfigPath = normalizePath(tsConfigPath);
    syncProjectProcessPaths(this.projectDir, this.tsConfigPath);
    this.tsModulePath = tsModulePath;
    this.documentRegistry = ts.createDocumentRegistry(ts.sys.useCaseSensitiveFileNames);
    this.overrides = new Map();
    this.deleted = new Set();
    this.actualPaths = new Map();
    this.pathAliases = new Map();
    this.versions = new Map();
    this.baseRoots = new Map();
    this.projectVersion = 0;
    this.configSignature = "";
    this.configInputs = new Map();
    this.configReads = undefined;
    this.fileSetSignature = "";
    this.parsedDiagnostics = [];
    this.parsed = undefined;
    this.service = undefined;
    this.rootLimit = undefined;
    this.rootLimitDisabled = false;
    this.transformerCache = new WeakMap();
    this.results = new Map();
    this.moduleFiles = new Map();
    this.warmFiles = undefined;
  }

  canonicalize(fileName) {
    const inputPath = path.normalize(path.resolve(path.isAbsolute(fileName) ? fileName : path.join(this.projectDir, fileName)));
    const resolved = this.pathAliases.get(inputPath) ?? normalizePath(inputPath);
    return this.ts.sys.useCaseSensitiveFileNames ? resolved : resolved.toLowerCase();
  }

  rememberFile(fileName) {
    const inputPath = path.normalize(path.resolve(path.isAbsolute(fileName) ? fileName : path.join(this.projectDir, fileName)));
    const actualPath = normalizePath(inputPath);
    this.pathAliases.set(inputPath, actualPath);
    const canonical = this.canonicalize(actualPath);
    if (!this.actualPaths.has(canonical)) {
      this.actualPaths.set(canonical, actualPath);
      this.versions.set(canonical, 0);
    }
    return this.actualPaths.get(canonical);
  }

  fileExists(fileName) {
    const canonical = this.canonicalize(fileName);
    if (this.deleted.has(canonical)) {
      return false;
    }
    if (this.overrides.has(canonical)) {
      return true;
    }
    return this.ts.sys.fileExists(this.actualPaths.get(canonical) ?? normalizePath(fileName));
  }

  readFile(fileName) {
    const canonical = this.canonicalize(fileName);
    if (this.deleted.has(canonical)) {
      return undefined;
    }
    if (this.overrides.has(canonical)) {
      return this.overrides.get(canonical);
    }
    return this.ts.sys.readFile(this.actualPaths.get(canonical) ?? normalizePath(fileName));
  }

  readConfigFile(fileName) {
    const actualPath = this.rememberFile(fileName);
    const text = this.readFile(actualPath);
    this.configReads?.set(this.canonicalize(actualPath), text);
    return text;
  }

  configFileExists(fileName) {
    const actualPath = this.rememberFile(fileName);
    const exists = this.fileExists(actualPath);
    this.configReads?.set(this.canonicalize(actualPath), exists ? this.readFile(actualPath) : undefined);
    return exists;
  }

  updateFile(fileName, text) {
    const actualPath = this.rememberFile(fileName);
    const canonical = this.canonicalize(actualPath);
    const unchanged = !this.deleted.has(canonical) && this.overrides.get(canonical) === text;
    this.deleted.delete(canonical);
    if (unchanged) {
      return actualPath;
    }
    this.overrides.set(canonical, text);
    this.versions.set(canonical, (this.versions.get(canonical) ?? 0) + 1);
    this.projectVersion += 1;
    return actualPath;
  }

  deleteFile(fileName) {
    const actualPath = this.rememberFile(fileName);
    const canonical = this.canonicalize(actualPath);
    if (this.deleted.has(canonical)) {
      return;
    }
    this.overrides.delete(canonical);
    this.deleted.add(canonical);
    this.versions.set(canonical, (this.versions.get(canonical) ?? 0) + 1);
    this.projectVersion += 1;
  }

  setBaseRoots(fileNames) {
    const next = new Map();
    for (const fileName of fileNames) {
      const actualPath = this.rememberFile(fileName);
      next.set(this.canonicalize(actualPath), actualPath);
    }
    const previousKeys = [...this.baseRoots.keys()];
    const changed = previousKeys.length !== next.size || previousKeys.some((key) => !next.has(key));
    this.baseRoots = next;
    if (changed) {
      this.projectVersion += 1;
    }
  }

  getScriptFileNames() {
    const roots = this.rootLimit ?? this.baseRoots;
    return [...roots.entries()]
      .filter(([canonical, actualPath]) => !this.deleted.has(canonical) && (this.overrides.has(canonical) || this.ts.sys.fileExists(actualPath)))
      .map(([, actualPath]) => actualPath);
  }

  setRootLimit(rootFileNames) {
    if (this.rootLimitDisabled) {
      return;
    }
    if (!Array.isArray(rootFileNames) || rootFileNames.length === 0) {
      this.rootLimitDisabled = true;
      if (this.rootLimit) {
        this.rootLimit = undefined;
        this.projectVersion += 1;
      }
      return;
    }
    const next = this.rootLimit ?? new Map();
    let widened = this.rootLimit === undefined;
    for (const fileName of rootFileNames) {
      const actualPath = this.rememberFile(fileName);
      const canonical = this.canonicalize(actualPath);
      if (!next.has(canonical)) {
        next.set(canonical, actualPath);
        widened = true;
      }
    }
    if (widened) {
      this.rootLimit = next;
      this.projectVersion += 1;
    }
  }

  getScriptVersion(fileName) {
    return this.versions.get(this.canonicalize(fileName)) ?? 0;
  }

  configInputsChanged() {
    if (!this.parsed) {
      return true;
    }
    for (const [canonical, previousText] of this.configInputs) {
      const actualPath = this.actualPaths.get(canonical) ?? canonical;
      if (this.readFile(actualPath) !== previousText) {
        return true;
      }
    }
    return false;
  }

  refreshParsedConfig(createService = true, requestedFileNames) {
    const requestedFileSet = Array.isArray(requestedFileNames)
      ? JSON.stringify(requestedFileNames.map((fileName) => this.canonicalize(fileName)).sort())
      : undefined;
    const shouldParse = this.configInputsChanged() || (requestedFileSet !== undefined && requestedFileSet !== this.fileSetSignature);

    if (shouldParse) {
      this.configReads = new Map();
      const parsed = this.ts.getParsedCommandLineOfConfigFile(this.tsConfigPath, {}, createParseHost(this, this.ts));
      this.configInputs = this.configReads;
      this.configReads = undefined;
      if (!parsed) {
        this.parsed = undefined;
        this.parsedDiagnostics = [createProtocolDiagnostic("error", "config-parse", `Failed to parse ${this.tsConfigPath}`)];
        return { diagnostics: this.parsedDiagnostics };
      }

      const configSignature = JSON.stringify({
        fileNames: parsed.fileNames.map((fileName) => this.canonicalize(fileName)).sort(),
        options: parsed.options,
        projectReferences: (parsed.projectReferences ?? []).map((reference) => reference.path),
      });
      if (!this.parsed || this.configSignature !== configSignature) {
        this.parsed = parsed;
        this.configSignature = configSignature;
        this.service?.dispose();
        this.service = undefined;
        this.transformerCache = new WeakMap();
        this.projectVersion += 1;
      } else {
        this.parsed = parsed;
      }
      this.parsedDiagnostics = parsed.errors.map((diagnostic) => toProtocolDiagnostic(this.ts, diagnostic));
      this.fileSetSignature = requestedFileSet ?? JSON.stringify(parsed.fileNames.map((fileName) => this.canonicalize(fileName)).sort());
      this.setBaseRoots(parsed.fileNames);
    }

    if (createService && this.parsed && !this.service) {
      this.service = this.ts.createLanguageService(createServiceHost(this, this.ts), this.documentRegistry);
    }
    return { diagnostics: this.parsedDiagnostics, parsed: this.parsed };
  }

  getSourceFile(program, fileName) {
    const actualPath = this.rememberFile(fileName);
    return program.getSourceFile(actualPath) ?? program.getSourceFiles().find((sourceFile) => this.canonicalize(sourceFile.fileName) === this.canonicalize(actualPath));
  }

  transformerList(program, configs) {
    const key = JSON.stringify(configs);
    let byConfig = this.transformerCache.get(program);
    if (!byConfig) {
      byConfig = new Map();
      this.transformerCache.set(program, byConfig);
    }
    let cached = byConfig.get(key);
    if (!cached) {
      cached = createTransformerList(this.ts, program, configs, this.projectDir, this.tsModulePath);
      byConfig.set(key, cached);
      this.trackModuleFiles(cached.moduleFiles);
    } else {
      resetPluginMetrics(cached.plugins);
    }
    return cached;
  }

  transformSourceFiles(program, sourceFiles, transforms) {
    const transformerList = wrapTransformersWithParentFix(this.ts, flattenIntoTransformers(transforms));
    if (transformerList.length === 0) {
      return { diagnostics: [], transformed: [] };
    }
    if (typeof this.ts.transformNodes !== "function") {
      return {
        diagnostics: [createProtocolDiagnostic("error", "transform-nodes-missing", "typescript.transformNodes is unavailable")],
        transformed: [],
      };
    }

    const result = this.ts.transformNodes(
      undefined,
      undefined,
      this.ts.factory,
      program.getCompilerOptions(),
      sourceFiles,
      transformerList,
      false,
    );
    const originalSourceFiles = new Set(sourceFiles);
    const transformedSourceFiles = result.transformed.filter((node) => this.ts.isSourceFile(node) && !originalSourceFiles.has(node));
    if (transformedSourceFiles.length === 0) {
      const diagnostics = (result.diagnostics ?? []).map((diagnostic) => toProtocolDiagnostic(this.ts, diagnostic));
      result.dispose?.();
      return { diagnostics, transformed: [] };
    }

    const printer = this.ts.createPrinter();
    const sources = new Map();
    const transformed = transformedSourceFiles.map((sourceFile) => {
      sources.set(this.canonicalize(sourceFile.fileName), sourceFile);
      return { fileName: sourceFile.fileName, text: printSourceFile(this.ts, printer, sourceFile) };
    });
    const resultHandle = crypto.randomBytes(16).toString("hex");
    this.results.set(resultHandle, { printer, result, sources });
    return {
      diagnostics: (result.diagnostics ?? []).map((diagnostic) => toProtocolDiagnostic(this.ts, diagnostic)),
      transformed,
      resultHandle,
    };
  }

  mapResult(resultHandle, fileNames) {
    const retained = this.results.get(resultHandle);
    if (!retained) {
      return invalidResultHandle(resultHandle);
    }
    const requested = fileNames ?? [...retained.sources.values()].map((sourceFile) => sourceFile.fileName);
    const diagnostics = [];
    const traceMaps = [];
    for (const fileName of requested) {
      const sourceFile = retained.sources.get(this.canonicalize(fileName));
      if (!sourceFile) {
        diagnostics.push(createProtocolDiagnostic("error", "result-file-missing", `Result ${resultHandle} does not contain ${fileName}`));
        continue;
      }
      traceMaps.push({ fileName: sourceFile.fileName, traceMap: printTraceMap(this.ts, retained.printer, sourceFile) });
    }
    return { diagnostics, transformed: [], traceMaps };
  }

  releaseResult(resultHandle) {
    const retained = this.results.get(resultHandle);
    if (!retained) {
      return invalidResultHandle(resultHandle);
    }
    retained.result.dispose?.();
    this.results.delete(resultHandle);
    return { diagnostics: [], transformed: [] };
  }

  trackModuleFiles(fileNames) {
    for (const fileName of fileNames ?? []) {
      if (!this.moduleFiles.has(fileName)) {
        this.moduleFiles.set(fileName, fileDigest(fileName));
      }
    }
  }

  hasStaleModules() {
    for (const [fileName, digest] of this.moduleFiles) {
      if (fileDigest(fileName) !== digest) {
        return true;
      }
    }
    return false;
  }

  rememberWarmFiles(program) {
    this.warmFiles = new Map();
    for (const sourceFile of program.getSourceFiles()) {
      const fileName = this.rememberFile(sourceFile.fileName);
      this.warmFiles.set(this.canonicalize(fileName), { fileName, stamp: fileStamp(fileName) });
    }
  }

  refreshWarmFiles(changedFiles) {
    if (!this.warmFiles) {
      return;
    }
    const changed = new Set(changedFiles.map((file) => this.canonicalize(file.fileName)));
    for (const [canonical, warmed] of this.warmFiles) {
      if (changed.has(canonical)) {
        continue;
      }
      const stamp = fileStamp(warmed.fileName);
      if (stamp === warmed.stamp) {
        continue;
      }
      if (stamp === undefined) {
        this.deleteFile(warmed.fileName);
      } else {
        const text = fs.readFileSync(warmed.fileName, "utf8");
        this.updateFile(warmed.fileName, text);
      }
    }
    this.warmFiles = undefined;
  }

  clearModuleCache() {
    for (const fileName of this.moduleFiles.keys()) {
      delete require.cache[fileName];
    }
  }

  dispose() {
    for (const retained of this.results.values()) {
      retained.result.dispose?.();
    }
    this.results.clear();
    this.service?.dispose();
    this.service = undefined;
  }

  handleRequest(request) {
    try {
      if (request.operation === "maps") {
        return this.mapResult(request.resultHandle, request.fileNames);
      }
      if (request.operation === "release") {
        return this.releaseResult(request.resultHandle);
      }
      if (request.operation === "validate") {
        const parsedState = this.refreshParsedConfig(false);
        if (!this.parsed) {
          return { diagnostics: parsedState.diagnostics, transformed: [] };
        }
        const pluginConfigs = Array.isArray(request.plugins) ? request.plugins : getPluginConfigs(this.parsed.options);
        const validation = validatePluginConfigs(pluginConfigs, this.projectDir, this.tsModulePath);
        this.trackModuleFiles(validation.moduleFiles);
        return {
          diagnostics: [...parsedState.diagnostics, ...validation.diagnostics],
          transformed: [],
          afterDeclarationsTransformers: validation.afterDeclarationsTransformers,
        };
      }

      if (request.operation === "warm") {
        const parsedState = this.refreshParsedConfig(true);
        if (!this.parsed) {
          return { diagnostics: parsedState.diagnostics, transformed: [] };
        }
        const program = this.service.getProgram();
        if (!program) {
          return {
            diagnostics: [...parsedState.diagnostics, createProtocolDiagnostic("error", "program-missing", "Language service did not return a program")],
            transformed: [],
          };
        }
        this.rememberWarmFiles(program);
        return { diagnostics: parsedState.diagnostics, transformed: [] };
      }

      this.refreshWarmFiles(request.changedFiles);
      for (const changedFile of request.changedFiles) {
        if (changedFile.deleted === true) {
          this.deleteFile(changedFile.fileName);
        } else {
          this.updateFile(changedFile.fileName, changedFile.text);
        }
      }
      const parsedState = this.refreshParsedConfig(true, request.fileNames);
      this.setRootLimit(request.rootFileNames);
      if (!this.parsed) {
        return { diagnostics: parsedState.diagnostics, transformed: [] };
      }
      const program = this.service.getProgram();
      if (!program) {
        return {
          diagnostics: [...parsedState.diagnostics, createProtocolDiagnostic("error", "program-missing", "Language service did not return a program")],
          transformed: [],
        };
      }

      const pluginConfigs = Array.isArray(request.plugins) ? request.plugins : getPluginConfigs(this.parsed.options);
      const transformerState = this.transformerList(program, pluginConfigs);
      const { transforms, diagnostics: pluginDiagnostics, plugins } = transformerState;
      const diagnostics = [...parsedState.diagnostics, ...pluginDiagnostics];
      const sourceFiles = [];
      for (const fileName of request.compileFileNames) {
        const sourceFile = this.getSourceFile(program, fileName);
        if (sourceFile) {
          sourceFiles.push(sourceFile);
        } else {
          diagnostics.push(createProtocolDiagnostic("error", "source-file-missing", `Source file not found in program: ${fileName}`));
        }
      }
      if (sourceFiles.length === 0) {
        return {
          diagnostics,
          transformed: [],
          metrics: { plugins: pluginMetrics(plugins) },
          afterDeclarationsTransformers: transforms.afterDeclarations.length,
        };
      }

      let transformResult;
      try {
        transformResult = this.transformSourceFiles(program, sourceFiles, transforms);
      } finally {
        for (const modulePath of transformerState.modulePaths) {
          this.trackModuleFiles(collectRequireClosure(modulePath));
        }
      }
      return {
        diagnostics: [...diagnostics, ...transformResult.diagnostics],
        transformed: transformResult.transformed,
        resultHandle: transformResult.resultHandle,
        metrics: { plugins: pluginMetrics(plugins) },
        afterDeclarationsTransformers: transforms.afterDeclarations.length,
      };
    } catch (error) {
      return { diagnostics: [createInternalDiagnostic(error)], transformed: [] };
    }
  }
}

function printSourceFile(ts, printer, sourceFile) {
  const writer = ts.createTextWriter("\n");
  printer.writeFile(sourceFile, writer);
  return writer.getText();
}

function printTraceMap(ts, printer, sourceFile) {
  const writer = ts.createTextWriter("\n");
  const generator = ts.createSourceMapGenerator(
    { getCurrentDirectory: () => "", getCanonicalFileName: (fileName) => fileName },
    sourceFile.fileName,
    "",
    "",
    {},
  );
  printer.writeFile(sourceFile, writer, generator);
  return JSON.stringify(generator.toJSON());
}

function invalidResultHandle(resultHandle) {
  return {
    diagnostics: [createProtocolDiagnostic("error", "invalid-result-handle", `Unknown or expired result handle: ${resultHandle}`)],
    transformed: [],
  };
}

function stringArray(value) {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function validateRequest(request) {
  if (!request || typeof request !== "object") {
    return createRequestDiagnostic("request must be a JSON object");
  }
  if (request.protocol !== 2) {
    return createRequestDiagnostic("protocol must equal 2");
  }
  if (typeof request.projectDir !== "string" || request.projectDir.length === 0) {
    return createRequestDiagnostic("projectDir must be a non-empty string");
  }
  if (typeof request.tsConfigPath !== "string" || request.tsConfigPath.length === 0) {
    return createRequestDiagnostic("tsConfigPath must be a non-empty string");
  }
  if (!["warm", "transform", "maps", "release", "validate"].includes(request.operation)) {
    return createRequestDiagnostic("operation must equal \"warm\", \"transform\", \"maps\", \"release\", or \"validate\"");
  }
  if (["transformSources", "emitDeclarations"].some((field) => request[field] !== undefined)) {
    return createRequestDiagnostic("protocol 2 does not accept protocol 1 request fields");
  }
  if (request.operation === "warm") {
    if (["fileNames", "rootFileNames", "compileFileNames", "changedFiles", "plugins"].some((field) => request[field] !== undefined)) {
      return createRequestDiagnostic("warm requests cannot include roots, overlays, or plugins");
    }
    return undefined;
  }
  if (request.operation === "validate") {
    return undefined;
  }
  if (request.operation === "maps") {
    if (typeof request.resultHandle !== "string" || request.resultHandle.length === 0) {
      return createRequestDiagnostic("maps requests must include a non-empty resultHandle");
    }
    if (request.fileNames !== undefined && !stringArray(request.fileNames)) {
      return createRequestDiagnostic("maps fileNames must be an array of strings when present");
    }
    return undefined;
  }
  if (request.operation === "release") {
    if (typeof request.resultHandle !== "string" || request.resultHandle.length === 0) {
      return createRequestDiagnostic("release requests must include a non-empty resultHandle");
    }
    if (!["success", "error", "cancel"].includes(request.outcome)) {
      return createRequestDiagnostic("release outcome must equal \"success\", \"error\", or \"cancel\"");
    }
    return undefined;
  }
  if (!stringArray(request.fileNames)) {
    return createRequestDiagnostic("fileNames must be a complete array of project source file names");
  }
  if (!stringArray(request.compileFileNames)) {
    return createRequestDiagnostic("compileFileNames must be an array of strings");
  }
  if (request.rootFileNames !== undefined && !stringArray(request.rootFileNames)) {
    return createRequestDiagnostic("rootFileNames must be an array of strings when present");
  }
  if (!Array.isArray(request.changedFiles)) {
    return createRequestDiagnostic("changedFiles must be an array");
  }
  for (const changedFile of request.changedFiles) {
    const hasText = typeof changedFile?.text === "string";
    const isDeleted = changedFile?.deleted === true;
    if (!changedFile || typeof changedFile.fileName !== "string" || hasText === isDeleted) {
      return createRequestDiagnostic("each changedFiles item must include string fileName and either string text or deleted: true");
    }
  }
  return undefined;
}

function captureRequestLogs(run) {
  const stdoutWrite = process.stdout.write;
  const stderrWrite = process.stderr.write;
  const chunks = [];

  // A plugin may terminate the process after printing its own diagnostic.
  // The normal response carries captured logs, but an immediate exit has no
  // response, so preserve those lines on the worker's stderr failure path.
  const flushOnExit = () => {
    stderrWrite.call(process.stderr, chunks.join(""));
  };
  process.once("exit", flushOnExit);
  const capture = (chunk, encoding, callback) => {
    chunks.push(Buffer.isBuffer(chunk) ? chunk.toString(typeof encoding === "string" ? encoding : undefined) : String(chunk));
    if (typeof encoding === "function") {
      encoding();
    } else if (typeof callback === "function") {
      callback();
    }
    return true;
  };
  process.stdout.write = capture;
  process.stderr.write = capture;
  try {
    let response;
    try {
      response = run();
    } catch (error) {
      response = { diagnostics: [createInternalDiagnostic(error)], transformed: [] };
    }
    return { response, logs: chunks.join("").split(/\r?\n/).filter(Boolean) };
  } finally {
    process.removeListener("exit", flushOnExit);
    process.stdout.write = stdoutWrite;
    process.stderr.write = stderrWrite;
  }
}

class SidecarServer {
  constructor(tsOrLoader) {
    this.loadTypeScript = typeof tsOrLoader === "function" ? tsOrLoader : () => tsOrLoader;
    this.session = undefined;
    this.sessionKey = "";
  }

  handleRequest(request) {
    const started = process.hrtime.bigint();
    const cpuStarted = process.cpuUsage();
    const { response, logs } = captureRequestLogs(() => this.handleRequestUnmetered(request));
    const cpu = process.cpuUsage(cpuStarted);
    response.logs = logs;
    response.metrics = {
      ...response.metrics,
      wallMs: Number((process.hrtime.bigint() - started) / 1000000n),
      cpuUserUs: cpu.user,
      cpuSystemUs: cpu.system,
      nodeVersion: process.version,
    };
    return response;
  }

  handleRequestUnmetered(request) {
    const validationError = validateRequest(request);
    if (validationError) {
      return { diagnostics: [validationError], transformed: [] };
    }

    const sessionKey = `${normalizePath(request.projectDir)}\u0000${normalizePath(request.tsConfigPath)}`;
    const requiresFreshnessCheck = request.operation === "warm" || request.operation === "transform" || request.operation === "validate";
    if (this.session && this.sessionKey === sessionKey && requiresFreshnessCheck && this.session.hasStaleModules()) {
      this.session.clearModuleCache();
      this.session.dispose();
      this.session = undefined;
      this.sessionKey = "";
    }

    if (request.operation === "maps" || request.operation === "release") {
      if (!this.session || this.sessionKey !== sessionKey) {
        return invalidResultHandle(request.resultHandle);
      }
      return this.session.handleRequest(request);
    }

    if (!this.session || this.sessionKey !== sessionKey) {
      this.session?.clearModuleCache();
      this.session?.dispose();
      let ts;
      let tsModulePath;
      try {
        ts = this.loadTypeScript(request.projectDir);
        tsModulePath = typeof this.loadTypeScript.modulePathFor === "function"
          ? this.loadTypeScript.modulePathFor(request.projectDir)
          : undefined;
      } catch (error) {
        return {
          diagnostics: [createProtocolDiagnostic(
            "error",
            "typescript-not-found",
            `Could not resolve the \`typescript\` package from ${request.projectDir}.\n` +
              `Transformer plugins require typescript in the project's node_modules (roblox-ts projects pin ~5.5.3).\n` +
              `More info: ${error instanceof Error ? error.message : String(error)}`,
          )],
          transformed: [],
        };
      }
      this.session = new SidecarProjectSession(ts, request.projectDir, request.tsConfigPath, tsModulePath);
      if (tsModulePath) {
        this.session.trackModuleFiles(collectRequireClosure(tsModulePath));
      }
      this.sessionKey = sessionKey;
    }
    return this.session.handleRequest(request);
  }

  close() {
    this.session?.clearModuleCache();
    this.session?.dispose();
    this.session = undefined;
    this.sessionKey = "";
  }
}

module.exports = { SidecarProjectSession, SidecarServer };
