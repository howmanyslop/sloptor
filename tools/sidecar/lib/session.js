const fs = require("node:fs");
const Module = require("node:module");
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
  validatePluginConfigs,
  wrapTransformersWithParentFix,
} = require("./plugins");

function measurePhase(phases, phase, run) {
  if (!phases) {
    return run();
  }
  const started = process.hrtime.bigint();
  try {
    return run();
  } finally {
    phases[phase] += process.hrtime.bigint() - started;
  }
}

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

class ProgramRequiredError extends Error {
  constructor(property) {
    super(`transformer read program.${String(property)}`);
    this.name = "ProgramRequiredError";
  }
}

class SidecarProjectSession {
  constructor(ts, projectDir, tsConfigPath, tsModulePath, benchOriginalReactProfile, reactSyntaxHost = false) {
    this.ts = ts;
    this.projectDir = normalizePath(projectDir);
    this.tsConfigPath = normalizePath(tsConfigPath);
    syncProjectProcessPaths(this.projectDir, this.tsConfigPath);
    this.tsModulePath = tsModulePath;
    // In-process BENCH/PRIVATE opt-in only; never read from the wire request.
    this.benchOriginalReactProfile = benchOriginalReactProfile;
    this.reactSyntaxHost = reactSyntaxHost;
    this.documentRegistry = ts.createDocumentRegistry(ts.sys.useCaseSensitiveFileNames);
    this.overrides = new Map();
    this.actualPaths = new Map();
    this.pathAliases = new Map();
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
    // programRequired latches once a plugin reads the program it was handed,
    // so later requests skip straight to the LanguageService.
    this.programRequired = false;
    // rebases holds, per canonical path, what this request sent to rebuild a
    // native jecs rewrite against the file on disk; see rebaseSourceFile.
    this.rebases = new Map();
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

  // parseOptionsOnly resolves the tsconfig's compiler options (extends chain
  // included) without expanding its include globs: the programless path needs
  // the options to parse and transform the files it is handed, never the
  // project's file list, and walking the globs is most of what parsing a large
  // project's config costs. An empty walk makes TypeScript report that the
  // config has no inputs, which is only true of this view of it.
  parseOptionsOnly() {
    const parsed = this.ts.getParsedCommandLineOfConfigFile(
      this.tsConfigPath,
      {},
      { ...createParseHost(this, this.ts), readDirectory: () => [] },
    );
    if (!parsed) {
      return undefined;
    }
    const noInputs = this.ts.Diagnostics.No_inputs_were_found_in_config_file_0_Specified_include_paths_were_1_and_exclude_paths_were_2.code;
    return { ...parsed, errors: parsed.errors.filter((diagnostic) => diagnostic.code !== noInputs) };
  }

  refreshParsedConfig(createService = true) {
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
      this.service?.dispose();
      this.parsed = parsed;
      this.configSignature = configSignature;
      this.service = undefined;
      this.projectVersion += 1;
    }

    if (createService && !this.service) {
      this.service = this.ts.createLanguageService(createServiceHost(this, this.ts), this.documentRegistry);
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

  // parseStandaloneSourceFile parses fileName the way the LanguageService's
  // document registry would, without building a program around it.
  parseStandaloneSourceFile(fileName, options) {
    const actualPath = this.rememberFile(fileName);
    const text = this.readFile(actualPath);
    if (text === undefined) {
      return undefined;
    }
    const languageOptions = {
      languageVersion: this.ts.getEmitScriptTarget(options),
      setExternalModuleIndicator: this.ts.getSetExternalModuleIndicator?.(options),
    };
    const sourceFile = this.ts.createSourceFile(actualPath, text, languageOptions, true);
    const rebase = this.rebases.get(this.canonicalize(actualPath));
    return rebase ? rebaseSourceFile(this.ts, sourceFile, rebase, languageOptions) : sourceFile;
  }

  // tryProgramlessTransform runs the request against source files parsed on
  // their own, handing every factory a program that refuses to be read. A
  // transformer that never looks at its program (the React-compiler applier
  // runs its own analysis) then costs no LanguageService parse and bind of the
  // project's import closure. The first time any plugin reads the program the
  // attempt is abandoned and the session stops trying, so the request is
  // answered by the ordinary path below with exactly the program it had before.
  tryProgramlessTransform(request, options, phases) {
    let touched = false;
    const program = new Proxy({}, {
      get(_target, property) {
        touched = true;
        throw new ProgramRequiredError(property);
      },
      has() {
        touched = true;
        throw new ProgramRequiredError("has");
      },
    });
    const pluginConfigs = Array.isArray(request.plugins) ? request.plugins : getPluginConfigs(options);
    let created;
    try {
      created = measurePhase(phases, "factoriesMs", () =>
        createTransformerList(this.ts, program, pluginConfigs, this.projectDir, this.tsModulePath));
    } catch (error) {
      if (touched) {
        return undefined;
      }
      throw error;
    }
    if (touched || created.diagnostics.length > 0) {
      return undefined;
    }
    const sourceFiles = [];
    for (const fileName of request.compileFileNames) {
      const sourceFile = this.parseStandaloneSourceFile(fileName, options);
      if (!sourceFile) {
        return undefined;
      }
      sourceFiles.push(sourceFile);
    }
    let transformResult;
    try {
      transformResult = this.transformSourceFilesWithOptions(options, sourceFiles, created.transforms, phases);
    } catch (error) {
      if (touched) {
        return undefined;
      }
      throw error;
    }
    if (touched) {
      return undefined;
    }
    return {
      diagnostics: transformResult.diagnostics,
      transformed: transformResult.transformed,
      metrics: { plugins: pluginMetrics(created.plugins) },
      afterDeclarationsTransformers: created.transforms.afterDeclarations.length,
    };
  }

  transformSourceFiles(program, sourceFiles, transforms, phases) {
    return this.transformSourceFilesWithOptions(program.getCompilerOptions(), sourceFiles, transforms, phases);
  }

  transformSourceFilesWithOptions(options, sourceFiles, transforms, phases) {
    const transformerList = wrapTransformersWithParentFix(
      this.ts,
      flattenIntoTransformers(transforms),
      phases ? (repair) => measurePhase(phases, "parentRepairMs", repair) : undefined,
    );
    // rbxtsc uses a bare intermediate printer. Compiler emit settings belong
    // to final emit, not to the text handed from one transformer to the next.
    const printer = this.ts.createPrinter();

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

    const result = measurePhase(phases, "transformMs", () => this.ts.transformNodes(
      undefined,
      undefined,
      this.ts.factory,
      options,
      sourceFiles,
      transformerList,
      false,
    ));

    try {
      return {
        diagnostics: (result.diagnostics ?? []).map((diagnostic) => toProtocolDiagnostic(this.ts, diagnostic)),
        transformed: measurePhase(phases, "printMs", () => result.transformed
          .filter((node) => this.ts.isSourceFile(node) && !sourceFiles.includes(node))
          .map((sourceFile) => ({
            fileName: sourceFile.fileName,
            ...printSourceFileWithTraceMap(this.ts, printer, sourceFile),
          }))),
      };
    } finally {
      if (typeof result.dispose === "function") {
        result.dispose();
      }
    }
  }

  handleRequest(request) {
    // Round only when responding so short repairs accumulate across the batch.
    const phases = { configMs: 0n, programMs: 0n, factoriesMs: 0n, transformMs: 0n, parentRepairMs: 0n, printMs: 0n };
    const respond = (response) => ({
      ...response,
      metrics: {
        ...response.metrics,
        phases: Object.fromEntries(Object.entries(phases).map(([phase, ns]) => [phase, Number(ns / 1000000n)])),
      },
    });
    try {
      if (request.operation === "warm") {
        // warm does a transform's fixed costs ahead of it, so a worker spawned
        // while rotor is still busy elsewhere is ready when the transform
        // arrives: TypeScript is already loaded with the session, and this
        // resolves the config's options and loads the plugin modules. It
        // reports nothing; the transform that follows reports for real.
        const parsed = measurePhase(phases, "configMs", () => this.parseOptionsOnly());
        if (parsed) {
          const pluginConfigs = Array.isArray(request.plugins) ? request.plugins : getPluginConfigs(parsed.options);
          measurePhase(phases, "factoriesMs", () => validatePluginConfigs(pluginConfigs, this.projectDir, this.tsModulePath));
        }
        return respond({ diagnostics: [], transformed: [] });
      }
      if (request.operation === "validate") {
        const parsedState = measurePhase(phases, "configMs", () => this.refreshParsedConfig(false));
        if (!this.parsed) {
          return respond({ diagnostics: parsedState.diagnostics, transformed: [] });
        }
        const pluginConfigs = Array.isArray(request.plugins) ? request.plugins : getPluginConfigs(this.parsed.options);
        const validation = measurePhase(phases, "factoriesMs", () => validatePluginConfigs(pluginConfigs, this.projectDir, this.tsModulePath));
        return respond({
          diagnostics: [...parsedState.diagnostics, ...validation.diagnostics],
          transformed: [],
          afterDeclarationsTransformers: validation.afterDeclarationsTransformers,
        });
      }

      this.rebases.clear();
      for (const changedFile of request.changedFiles) {
        const actualPath = this.updateFile(changedFile.fileName, changedFile.text);
        if (changedFile.rebase) {
          this.rebases.set(this.canonicalize(actualPath), changedFile.rebase);
        }
      }

      if (!this.programRequired && request.compileFileNames.length > 0) {
        const parsed = measurePhase(phases, "configMs", () => this.parseOptionsOnly());
        if (!parsed) {
          return respond({
            diagnostics: [createProtocolDiagnostic("error", "config-parse", `Failed to parse ${this.tsConfigPath}`)],
            transformed: [],
          });
        }
        if (parsed.errors.length === 0) {
          const programless = this.tryProgramlessTransform(request, parsed.options, phases);
          if (programless) {
            return respond(programless);
          }
        }
        this.programRequired = true;
      }

      const parsedState = measurePhase(phases, "configMs", () => {
        const state = this.refreshParsedConfig(!this.benchOriginalReactProfile && !this.reactSyntaxHost);
        this.setRootLimit(request.rootFileNames);
        return state;
      });
      if (!this.parsed) {
        return respond({ diagnostics: parsedState.diagnostics, transformed: [] });
      }

      const pluginConfigs = Array.isArray(request.plugins) ? request.plugins : getPluginConfigs(this.parsed.options);
      if (this.reactSyntaxHost && this.reactSyntaxProfileSignature !== this.configSignature) {
        this.reactSyntaxProfile = require("./react-syntax-profile").resolveReactSyntaxProfile(
          this.projectDir, this.tsModulePath, this.parsed.options.plugins);
        this.reactSyntaxProfileSignature = this.configSignature;
      }
      const profile = this.benchOriginalReactProfile ?? this.reactSyntaxProfile;
      let syntax;
      if (profile) {
        const { BENCH_ORIGINAL_SOURCE, selectOriginalReactInputs } = require("./original-react-syntax");
        // Only the initial authored native stage may claim this wire value.
        // A JSON request cannot provide the private Symbol itself.
        const input = !this.benchOriginalReactProfile && this.reactSyntaxHost && request.sourceOrigin === "original-authored"
          ? { ...request, [BENCH_ORIGINAL_SOURCE]: profile } : request;
        syntax = selectOriginalReactInputs(this, input, pluginConfigs, createParseHost(this, this.ts), profile);
      }
      let program;
      if (syntax) {
        // The next generic request recreates a service from the same accumulated
        // overlays, versions, config and monotonically widening root state.
        this.service?.dispose();
        this.service = undefined;
      } else {
        if (!this.service) {
          this.service = this.ts.createLanguageService(createServiceHost(this, this.ts), this.documentRegistry);
        }
        program = measurePhase(phases, "programMs", () => this.service.getProgram());
      }
      if (!syntax && !program) {
        return respond({
          diagnostics: [...parsedState.diagnostics, createProtocolDiagnostic("error", "program-missing", "Language service did not return a program")],
          transformed: [],
        });
      }

      const { transforms, diagnostics: pluginDiagnostics, plugins } = measurePhase(phases, "factoriesMs", () =>
        createTransformerList(this.ts, program, pluginConfigs, this.projectDir, this.tsModulePath, syntax?.entryPath));

      const diagnostics = [...parsedState.diagnostics, ...pluginDiagnostics];
      const sourceFiles = syntax
        ? require("./original-react-syntax").parseOriginalReactInputs(this, syntax.inputs)
        : [];
      for (const fileName of syntax ? [] : request.compileFileNames) {
        const sourceFile = this.getSourceFile(program, fileName);
        if (!sourceFile) {
          diagnostics.push(createProtocolDiagnostic("error", "source-file-missing", `Source file not found in program: ${fileName}`));
          continue;
        }
        sourceFiles.push(sourceFile);
      }

      if (sourceFiles.length === 0) {
        return respond({ diagnostics, transformed: [], metrics: { plugins: pluginMetrics(plugins) }, afterDeclarationsTransformers: transforms.afterDeclarations.length });
      }

      const transformResult = syntax
        ? this.transformSourceFilesWithOptions(this.parsed.options, sourceFiles, transforms, phases)
        : this.transformSourceFiles(program, sourceFiles, transforms, phases);
      return respond({
        diagnostics: [...diagnostics, ...transformResult.diagnostics],
        transformed: transformResult.transformed,
        metrics: { plugins: pluginMetrics(plugins) },
        // Rotor emits declarations natively (tsgo has no afterDeclarations
        // hook), so the count is reported rather than run: rotor turns a
        // non-zero value into a one-shot warning for the project.
        afterDeclarationsTransformers: transforms.afterDeclarations.length,
      });
    } catch (error) {
      return respond({
        diagnostics: [createInternalDiagnostic(error)],
        transformed: [],
      });
    }
  }

  dispose() {
    this.service?.dispose();
    this.service = undefined;
  }
}

// rebaseSourceFile turns the parse of a native jecs rewrite back into the tree
// the JavaScript jecs transformer hands the next plugin: the file on disk,
// holding the rewrite's statements, each node at the range it was written at
// there and every node jecs made up at none. Plugins that map their edits back
// through node ranges, and the trace printed afterwards, then land where they
// land under the JavaScript transformer. rebase.ranges lists, per node of the
// rewrite in preorder, its start and end in the rewrite, its range on disk and
// whether a factory made it (internal/compile/jecs_rebase.go). A node the list
// does not name, such as a token tsgo keeps no node for, counts as made up.
function rebaseSourceFile(ts, printed, rebase, languageOptions) {
  const original = ts.createSourceFile(printed.fileName, rebase.text, languageOptions, true);
  const { ranges } = rebase;
  const nodesAt = new Map();
  for (let index = 0; index + 4 < ranges.length; index += 5) {
    const key = `${ranges[index]}:${ranges[index + 1]}`;
    const queue = nodesAt.get(key);
    if (queue) {
      queue.push(index);
    } else {
      nodesAt.set(key, [index]);
    }
  }
  const visit = (node) => {
    const index = nodesAt.get(`${node.getStart(printed)}:${node.end}`)?.shift();
    ts.forEachChild(node, visit);
    if (index === undefined) {
      node.pos = -1;
      node.end = -1;
      node.flags |= ts.NodeFlags.Synthesized;
      return;
    }
    node.pos = ranges[index + 2];
    node.end = ranges[index + 3];
    if (ranges[index + 4] === 1) {
      node.flags |= ts.NodeFlags.Synthesized;
    }
  };
  for (const statement of printed.statements) {
    visit(statement);
  }
  return ts.factory.updateSourceFile(original, printed.statements);
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
  if (request.operation !== "transform" && request.operation !== "validate" && request.operation !== "warm") {
    return createRequestDiagnostic("operation must equal \"transform\", \"validate\" or \"warm\"");
  }
  if (request.operation === "validate" || request.operation === "warm") {
    return undefined;
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
    const { rebase } = changedFile;
    if (rebase !== undefined && (!rebase || typeof rebase.text !== "string" || !Array.isArray(rebase.ranges))) {
      return createRequestDiagnostic("a changedFiles rebase must include string text and an array of ranges");
    }
  }
  return undefined;
}

// projectModulePaths matches the NODE_PATH the CLI gives its sidecar
// (sidecarEnv in internal/compile/sidecar.go): each node_modules above the
// project, then the sidecar's own.
function projectModulePaths(projectDir) {
  const paths = [];
  for (let dir = path.resolve(projectDir); ; dir = path.dirname(dir)) {
    paths.push(path.join(dir, "node_modules"));
    if (path.dirname(dir) === dir) {
      break;
    }
  }
  paths.push(path.join(__dirname, "..", "node_modules"));
  return paths.filter((dir) => fs.existsSync(dir));
}

// withProjectModulePaths gives bare requests a NODE_PATH-style fallback for
// one request only, so an in-process host (the JS API) resolves plugins like
// the CLI sidecar without changing module resolution for the rest of its
// process. NODE_PATH itself is read once at Node startup.
function withProjectModulePaths(projectDir, run) {
  const paths = projectModulePaths(projectDir);
  const resolveFilename = Module._resolveFilename;
  Module._resolveFilename = function (request, parent, isMain, options) {
    try {
      return resolveFilename.call(this, request, parent, isMain, options);
    } catch (error) {
      if (error?.code !== "MODULE_NOT_FOUND" || path.isAbsolute(request) || request.startsWith(".")) {
        throw error;
      }
      try {
        return resolveFilename.call(this, request, parent, isMain, { paths });
      } catch {
        throw error;
      }
    }
  };
  try {
    return run();
  } finally {
    Module._resolveFilename = resolveFilename;
  }
}

class SidecarServer {
  constructor(tsOrLoader, { benchOriginalReactProfile, reactSyntaxHost = false } = {}) {
    this.loadTypeScript = typeof tsOrLoader === "function" ? tsOrLoader : () => tsOrLoader;
    this.benchOriginalReactProfile = benchOriginalReactProfile;
    this.reactSyntaxHost = reactSyntaxHost;
    this.sessions = new Map();
  }

  handleRequest(request) {
    const started = process.hrtime.bigint();
    const cpuStarted = process.cpuUsage();
    const response = typeof request?.projectDir === "string" && request.projectDir.length > 0
      ? withProjectModulePaths(request.projectDir, () => this.handleRequestUnmetered(request))
      : this.handleRequestUnmetered(request);
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

    const previousCwd = process.cwd();
    const previousArgv = [...process.argv];
    try {
      syncProjectProcessPaths(request.projectDir, request.tsConfigPath);
      const canonicalPath = (fileName) => {
        const normalized = normalizePath(fileName);
        return process.platform === "win32" ? normalized.toLowerCase() : normalized;
      };
      const sessionKey = `${canonicalPath(request.projectDir)}\u0000${canonicalPath(request.tsConfigPath)}`;
      let session = this.sessions.get(sessionKey);
      if (!session) {
        let ts;
        let tsModulePath;
        try {
          ts = this.loadTypeScript(request.projectDir);
          if (typeof this.loadTypeScript.modulePathFor === "function") {
            tsModulePath = this.loadTypeScript.modulePathFor(request.projectDir);
          }
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
        session = new SidecarProjectSession(ts, request.projectDir, request.tsConfigPath, tsModulePath, this.benchOriginalReactProfile, this.reactSyntaxHost);
        this.sessions.set(sessionKey, session);
      }

      syncProjectProcessPaths(session.projectDir, session.tsConfigPath);
      return session.handleRequest(request);
    } finally {
      process.chdir(previousCwd);
      process.argv.splice(0, process.argv.length, ...previousArgv);
    }
  }

  dispose() {
    for (const session of this.sessions.values()) {
      session.dispose();
    }
    this.sessions.clear();
  }
}

module.exports = {
  SidecarProjectSession,
  SidecarServer,
};
