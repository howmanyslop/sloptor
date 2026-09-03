const path = require("node:path");
const {
  createInternalDiagnostic,
  createProtocolDiagnostic,
  createRequestDiagnostic,
  toProtocolDiagnostic,
} = require("./diagnostics");
const {
  createTransformerList,
  flattenIntoTransformers,
  pluginMetrics,
  getPluginConfigs,
  wrapTransformersWithParentFix,
} = require("./plugins");

function createParseHost(session, ts) {
  return {
    useCaseSensitiveFileNames: ts.sys.useCaseSensitiveFileNames,
    readDirectory: ts.sys.readDirectory,
    fileExists: (fileName) => session.fileExists(fileName),
    readFile: (fileName) => session.readFile(fileName),
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
    getProjectReferences: () => session.parsed.projectReferences,
    getScriptFileNames: () => session.getScriptFileNames(),
    getScriptSnapshot: (fileName) => {
      const text = session.readFile(fileName);
      if (text === undefined) {
        return undefined;
      }

      return ts.ScriptSnapshot.fromString(text);
    },
    getScriptVersion: (fileName) => String(session.getScriptVersion(fileName)),
    readDirectory: ts.sys.readDirectory,
    readFile: (fileName) => session.readFile(fileName),
    realpath: ts.sys.realpath ? (fileName) => ts.sys.realpath(fileName) : undefined,
    useCaseSensitiveFileNames: () => ts.sys.useCaseSensitiveFileNames,
  };
}

function normalizePath(fileName) {
  return path.normalize(path.resolve(fileName));
}

class SidecarProjectSession {
  constructor(ts, projectDir, tsConfigPath) {
    this.ts = ts;
    this.projectDir = normalizePath(projectDir);
    this.tsConfigPath = normalizePath(tsConfigPath);
    this.documentRegistry = ts.createDocumentRegistry(ts.sys.useCaseSensitiveFileNames);
    this.overrides = new Map();
    this.actualPaths = new Map();
    this.versions = new Map();
    this.projectVersion = 0;
    this.configSignature = "";
    // rootLimit is the canonical root set narrowed requests have asked for so
    // far, or undefined for "every file the tsconfig names". TypeScript still
    // pulls each root's import closure into the program, so a narrowed program
    // answers the same questions about the files being compiled while parsing
    // and binding far fewer of them.
    this.rootLimit = undefined;
    // rootLimitDisabled records that the limit has already grown to every file
    // the tsconfig names, which is what a request without a limit asks for.
    // Nothing can widen it further, so later narrowed requests are no-ops.
    this.rootLimitDisabled = false;
    this.parsed = undefined;
    this.service = undefined;
  }

  canonicalize(fileName) {
    const resolved = normalizePath(path.isAbsolute(fileName) ? fileName : path.join(this.projectDir, fileName));
    return this.ts.sys.useCaseSensitiveFileNames ? resolved : resolved.toLowerCase();
  }

  rememberFile(fileName) {
    const actualPath = normalizePath(path.isAbsolute(fileName) ? fileName : path.join(this.projectDir, fileName));
    const canonical = this.canonicalize(actualPath);
    if (!this.actualPaths.has(canonical)) {
      this.actualPaths.set(canonical, actualPath);
      this.versions.set(canonical, 0);
      this.projectVersion += 1;
    }

    return this.actualPaths.get(canonical);
  }

  fileExists(fileName) {
    const canonical = this.canonicalize(fileName);
    if (this.overrides.has(canonical)) {
      return true;
    }

    const actualPath = this.actualPaths.get(canonical) ?? normalizePath(fileName);
    return this.ts.sys.fileExists(actualPath);
  }

  readFile(fileName) {
    const canonical = this.canonicalize(fileName);
    if (this.overrides.has(canonical)) {
      return this.overrides.get(canonical);
    }

    const actualPath = this.actualPaths.get(canonical) ?? normalizePath(fileName);
    return this.ts.sys.readFile(actualPath);
  }

  updateFile(fileName, text) {
    const actualPath = this.rememberFile(fileName);
    const canonical = this.canonicalize(actualPath);
    if (this.overrides.get(canonical) === text) {
      return actualPath;
    }
    this.overrides.set(canonical, text);
    this.versions.set(canonical, (this.versions.get(canonical) ?? 0) + 1);
    this.projectVersion += 1;
    return actualPath;
  }

  getScriptFileNames() {
    const fileNames = [];
    for (const [canonical, actualPath] of this.actualPaths.entries()) {
      if (this.rootLimit && !this.rootLimit.has(canonical)) {
        continue;
      }
      if (this.overrides.has(canonical) || this.ts.sys.fileExists(actualPath)) {
        fileNames.push(actualPath);
      }
    }
    return fileNames;
  }

  // setRootLimit widens the LanguageService root set toward what this request
  // needs. The limit only ever grows within a session: a watch session rebuilds
  // through the same warm worker, and shrinking the root set back down between
  // rebuilds would throw away the program TypeScript had just built and force a
  // fresh parse and bind of the files that came back. Growing it only adds
  // files, which the incremental program update already handles.
  //
  // rootLimitDisabled is that same monotonicity, encoded cheaply for the one
  // case where the limit has grown as far as it can go. A request with no limit
  // means "every file the tsconfig names", so after it the effective root set
  // IS the full remembered set — the widest this session can ever be. The latch
  // says so in one boolean instead of materializing that set and re-checking it
  // on every later request. It is not a separate policy, and dropping it alone
  // would break the monotonicity rather than relax it: `rootLimit` is undefined
  // once the limit is gone, so the next narrowed request would seed a fresh Set
  // from its own file list and SHRINK the program to it.
  //
  // The project version has to move whenever the root set does: the
  // compilation settings and every file version are unchanged, so nothing else
  // would tell the service that its program is stale.
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
    const next = this.rootLimit ?? new Set();
    let widened = this.rootLimit === undefined;
    for (const fileName of rootFileNames) {
      const canonical = this.canonicalize(this.rememberFile(fileName));
      if (!next.has(canonical)) {
        next.add(canonical);
        widened = true;
      }
    }
    if (!widened) {
      return;
    }
    this.rootLimit = next;
    this.projectVersion += 1;
  }

  getScriptVersion(fileName) {
    const canonical = this.canonicalize(fileName);
    return this.versions.get(canonical) ?? 0;
  }

  refreshParsedConfig() {
    const parsed = this.ts.getParsedCommandLineOfConfigFile(
      this.tsConfigPath,
      {},
      createParseHost(this, this.ts),
    );
    if (!parsed) {
      return {
        diagnostics: [createProtocolDiagnostic("error", "config-parse", `Failed to parse ${this.tsConfigPath}`)],
      };
    }

    for (const fileName of parsed.fileNames) {
      this.rememberFile(fileName);
    }

    const configSignature = JSON.stringify({
      fileNames: [...parsed.fileNames].sort(),
      options: parsed.options,
      projectReferences: (parsed.projectReferences ?? []).map((reference) => reference.path),
    });

    if (!this.parsed || this.configSignature !== configSignature) {
      this.parsed = parsed;
      this.configSignature = configSignature;
      this.service = this.ts.createLanguageService(createServiceHost(this, this.ts), this.documentRegistry);
      this.projectVersion += 1;
    }

    return {
      diagnostics: parsed.errors.map((diagnostic) => toProtocolDiagnostic(this.ts, diagnostic)),
      parsed,
    };
  }

  getSourceFile(program, fileName) {
    const actualPath = this.rememberFile(fileName);
    const direct = program.getSourceFile(actualPath);
    if (direct) {
      return direct;
    }

    const canonical = this.canonicalize(actualPath);
    return program.getSourceFiles().find((sourceFile) => this.canonicalize(sourceFile.fileName) === canonical);
  }

  transformSourceFiles(program, sourceFiles, transforms) {
    const transformerList = wrapTransformersWithParentFix(this.ts, flattenIntoTransformers(transforms));
    const printer = this.ts.createPrinter({ removeComments: program.getCompilerOptions().removeComments });

    if (transformerList.length === 0) {
      return {
        diagnostics: [],
        transformed: [],
      };
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

    try {
      return {
        diagnostics: (result.diagnostics ?? []).map((diagnostic) => toProtocolDiagnostic(this.ts, diagnostic)),
        transformed: result.transformed
          .filter((node) => this.ts.isSourceFile(node) && !sourceFiles.includes(node))
          .map((sourceFile) => ({
            fileName: sourceFile.fileName,
            ...printSourceFileWithTraceMap(this.ts, printer, sourceFile),
          })),
      };
    } finally {
      if (typeof result.dispose === "function") {
        result.dispose();
      }
    }
  }

  handleRequest(request) {
    try {
      for (const changedFile of request.changedFiles) {
        this.updateFile(changedFile.fileName, changedFile.text);
      }

      const parsedState = this.refreshParsedConfig();
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
      const { transforms, diagnostics: pluginDiagnostics, plugins } = createTransformerList(this.ts, program, pluginConfigs, this.projectDir);

      const diagnostics = [...parsedState.diagnostics, ...pluginDiagnostics];
      const sourceFiles = [];
      for (const fileName of request.compileFileNames) {
        const sourceFile = this.getSourceFile(program, fileName);
        if (!sourceFile) {
          diagnostics.push(createProtocolDiagnostic("error", "source-file-missing", `Source file not found in program: ${fileName}`));
          continue;
        }
        sourceFiles.push(sourceFile);
      }

      if (sourceFiles.length === 0) {
        return { diagnostics, transformed: [], metrics: { plugins: pluginMetrics(plugins) }, afterDeclarationsTransformers: transforms.afterDeclarations.length };
      }

      const transformResult = request.transformSources
        ? this.transformSourceFiles(program, sourceFiles, transforms)
        : { diagnostics: [], transformed: [] };
      return {
        diagnostics: [...diagnostics, ...transformResult.diagnostics],
        transformed: transformResult.transformed,
        metrics: { plugins: pluginMetrics(plugins) },
        // Rotor emits declarations natively (tsgo has no afterDeclarations
        // hook), so the count is reported rather than run: rotor turns a
        // non-zero value into a one-shot warning for the project.
        afterDeclarationsTransformers: transforms.afterDeclarations.length,
      };
    } catch (error) {
      return {
        diagnostics: [createInternalDiagnostic(error)],
        transformed: [],
      };
    }
  }
}

// The trace map is not an emit artifact — rotor needs it to put every position
// this reprint produces (diagnostics included) back into the coordinates of the
// file on disk. So it is generated whether or not the project emits source
// maps; a project that turned sourceMap off would otherwise report every
// diagnostic at an offset into text nobody can see.
function printSourceFileWithTraceMap(ts, printer, sourceFile) {
  const writer = ts.createTextWriter("\n");
  const generator = ts.createSourceMapGenerator(
    {
      getCurrentDirectory: () => "",
      getCanonicalFileName: (fileName) => fileName,
    },
    sourceFile.fileName,
    "",
    "",
    {},
  );
  printer.writeFile(sourceFile, writer, generator);
  return { text: writer.getText(), traceMap: JSON.stringify(generator.toJSON()) };
}

function validateRequest(request) {
  if (!request || typeof request !== "object") {
    return createRequestDiagnostic("request must be a JSON object");
  }
  if (request.protocol !== 1) {
    return createRequestDiagnostic("protocol must equal 1");
  }
  if (typeof request.projectDir !== "string" || request.projectDir.length === 0) {
    return createRequestDiagnostic("projectDir must be a non-empty string");
  }
  if (typeof request.tsConfigPath !== "string" || request.tsConfigPath.length === 0) {
    return createRequestDiagnostic("tsConfigPath must be a non-empty string");
  }
  if (!Array.isArray(request.compileFileNames) || !request.compileFileNames.every((fileName) => typeof fileName === "string")) {
    return createRequestDiagnostic("compileFileNames must be an array of strings");
  }
  if (request.rootFileNames !== undefined && (!Array.isArray(request.rootFileNames) || !request.rootFileNames.every((fileName) => typeof fileName === "string"))) {
    return createRequestDiagnostic("rootFileNames must be an array of strings when present");
  }
  if (!Array.isArray(request.changedFiles)) {
    return createRequestDiagnostic("changedFiles must be an array");
  }
  for (const changedFile of request.changedFiles) {
    if (!changedFile || typeof changedFile.fileName !== "string" || typeof changedFile.text !== "string") {
      return createRequestDiagnostic("each changedFiles item must include string fileName and text");
    }
  }
  if (typeof request.transformSources !== "boolean") {
    return createRequestDiagnostic("transformSources must be a boolean");
  }
  return undefined;
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
    const response = this.handleRequestUnmetered(request);
    const cpu = process.cpuUsage(cpuStarted);
    const wallNs = process.hrtime.bigint() - started;
    response.metrics = {
      ...response.metrics,
      wallMs: Number(wallNs / 1000000n),
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
    if (!this.session || this.sessionKey !== sessionKey) {
      let ts;
      try {
        ts = this.loadTypeScript(request.projectDir);
      } catch (error) {
        return {
          diagnostics: [
            createProtocolDiagnostic(
              "error",
              "typescript-not-found",
              `Could not resolve the \`typescript\` package from ${request.projectDir}.\n` +
                `Transformer plugins require typescript in the project's node_modules (roblox-ts projects pin ~5.5.3).\n` +
                `More info: ${error instanceof Error ? error.message : String(error)}`,
            ),
          ],
          transformed: [],
        };
      }
      this.session = new SidecarProjectSession(ts, request.projectDir, request.tsConfigPath);
      this.sessionKey = sessionKey;
    }

    return this.session.handleRequest(request);
  }
}

module.exports = {
  SidecarProjectSession,
  SidecarServer,
};
