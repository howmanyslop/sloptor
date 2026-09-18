const fs = require("node:fs");
const path = require("node:path");
const { createPluginNotFoundDiagnostic, createProtocolDiagnostic } = require("./diagnostics");

// getPluginConfigs reads the transformer list off the RESOLVED compiler
// options — the value `tsc --showConfig` reports — rather than walking the
// `extends` chain itself. `plugins` is an array-valued compiler option, so
// TypeScript's own resolution has already applied the rule that a child which
// declares `plugins` replaces the parent's list instead of adding to it. A
// child can therefore drop an inherited transform with `"plugins": []`.
//
// rbxtsc concatenates every config in the chain (Project/transformers/
// getPluginConfigs.ts), which leaves no way to opt out; rotor diverges here
// deliberately. See "Deliberate Divergence" in docs/sidecar-protocol.md.
function getPluginConfigs(options) {
  const plugins = options?.plugins;
  if (!Array.isArray(plugins)) {
    return [];
  }

  return plugins.filter((pluginConfig) => pluginConfig && typeof pluginConfig.transform === "string");
}

function getTransformerFromFactory(ts, factory, config, program) {
  const { after, afterDeclarations, type, ...manualConfig } = config;
  let transformer;

  switch (type) {
    case undefined:
    case "program":
      transformer = factory(program, manualConfig, { ts });
      break;
    case "checker":
      transformer = factory(program.getTypeChecker(), manualConfig);
      break;
    case "compilerOptions":
      transformer = factory(program.getCompilerOptions(), manualConfig);
      break;
    case "config":
      transformer = factory(manualConfig);
      break;
    case "raw":
      transformer = (context) => factory(context, program, manualConfig);
      break;
    default:
      return undefined;
  }

  if (typeof transformer === "function") {
    if (after) {
      return { after: transformer };
    }
    if (afterDeclarations) {
      return { afterDeclarations: transformer };
    }
    return { before: transformer };
  }

  return transformer;
}

function realpath(fileName) {
  return fs.realpathSync.native?.(fileName) ?? fs.realpathSync(fileName);
}

function collectRequireClosure(modulePath) {
  const files = new Set();
  const visit = (loadedModule) => {
    if (!loadedModule || files.has(loadedModule.filename)) {
      return;
    }
    files.add(loadedModule.filename);
    for (const child of loadedModule.children) {
      visit(child);
    }
  };
  visit(require.cache[modulePath]);
  if (files.size === 0) {
    files.add(modulePath);
  }
  return files;
}

function loadPluginDefinition(config, baseDir, tsModulePath) {
  const modulePath = require.resolve(config.transform, { paths: [baseDir] });
  const requiredModule = require(modulePath);
  const factoryModule = typeof requiredModule === "function"
    ? Object.assign({ default: requiredModule }, requiredModule)
    : requiredModule;
  const factory = factoryModule[config.import ?? "default"];
  const moduleFiles = collectRequireClosure(modulePath);

  if (typeof factory !== "function") {
    const error = new Error("factory not a function");
    error.moduleFiles = moduleFiles;
    throw error;
  }

  if (tsModulePath) {
    let pluginTypeScriptPath;
    try {
      pluginTypeScriptPath = realpath(require.resolve("typescript", { paths: [path.dirname(modulePath)] }));
    } catch (error) {
      if (error?.code !== "MODULE_NOT_FOUND") {
        throw error;
      }
    }
    if (pluginTypeScriptPath && pluginTypeScriptPath !== tsModulePath) {
      moduleFiles.add(pluginTypeScriptPath);
      return {
        diagnostic: createProtocolDiagnostic(
          "error",
          "typescript-instance-mismatch",
          `Transformer \`${config.transform}\` resolves TypeScript from ${pluginTypeScriptPath}, but the project uses ${tsModulePath}. Install one shared TypeScript copy so transformer nodes and the sidecar use the same module instance.`,
        ),
        moduleFiles,
      };
    }
  }

  return { modulePath, factoryModule, factory, moduleFiles };
}

// validatePluginConfigs performs only the protocol's declaration-time checks:
// resolve the configured module, load it, confirm its exported factory, and
// ensure it shares the session's TypeScript installation. It deliberately does
// not call a factory, including raw factories, because validation can run for a
// declaration-only build where no source transformation is needed.
function validatePluginConfigs(configs, baseDir, tsModulePath) {
  const diagnostics = [];
  let afterDeclarationsTransformers = 0;
  const moduleFiles = new Set();

  for (const config of configs) {
    if (!config.transform) {
      continue;
    }

    try {
      const loaded = loadPluginDefinition(config, baseDir, tsModulePath);
      for (const fileName of loaded.moduleFiles) {
        moduleFiles.add(fileName);
      }
      if (loaded.diagnostic) {
        diagnostics.push(loaded.diagnostic);
        continue;
      }
      if (config.afterDeclarations) {
        afterDeclarationsTransformers += 1;
      }
    } catch (error) {
      for (const fileName of error?.moduleFiles ?? []) {
        moduleFiles.add(fileName);
      }
      diagnostics.push(createPluginNotFoundDiagnostic(config.transform, error));
    }
  }

  return { diagnostics, afterDeclarationsTransformers, moduleFiles };
}

function wrapWithShouldTransform(ts, transformer, shouldTransformSourceFile, program, config) {
  if (typeof shouldTransformSourceFile !== "function") {
    return transformer;
  }

  return (context) => {
    const transform = transformer(context);
    return (sourceFile) => {
      if (!ts.isSourceFile(sourceFile) || shouldTransformSourceFile(sourceFile, program, config)) {
        return transform(sourceFile);
      }
      return sourceFile;
    };
  };
}

// wrapWithTiming charges a plugin for the wall time its factory and every
// node visit costs, so a slow round trip can be split across the plugins that
// share it rather than reported as one opaque total.
function wrapWithTiming(transformer, plugin) {
  return (context) => {
    const factoryStarted = process.hrtime.bigint();
    const transform = transformer(context);
    plugin.ns += process.hrtime.bigint() - factoryStarted;
    return (node) => {
      const started = process.hrtime.bigint();
      try {
        return transform(node);
      } finally {
        plugin.ns += process.hrtime.bigint() - started;
      }
    };
  };
}

// pluginMetrics snapshots the accumulator createTransformerList hands back,
// which only holds its final values once the transforms have run.
function pluginMetrics(plugins) {
  return plugins.map((plugin) => ({ transform: plugin.transform, ms: Number(plugin.ns / 1000000n) }));
}

function resetPluginMetrics(plugins) {
  for (const plugin of plugins) {
    plugin.ns = 0n;
  }
}

function createTransformerList(ts, program, configs, baseDir, tsModulePath) {
  const transforms = {
    before: [],
    after: [],
    afterDeclarations: [],
  };
  const diagnostics = [];
  const plugins = [];
  const moduleFiles = new Set();
  const modulePaths = new Set();

  for (const config of configs) {
    if (!config.transform) {
      continue;
    }

    try {
      const loaded = loadPluginDefinition(config, baseDir, tsModulePath);
      for (const fileName of loaded.moduleFiles) {
        moduleFiles.add(fileName);
      }
      if (loaded.diagnostic) {
        diagnostics.push(loaded.diagnostic);
        continue;
      }
      modulePaths.add(loaded.modulePath);
      const plugin = { transform: config.transform, ns: 0n };
      const factoryStarted = process.hrtime.bigint();
      let transformer;
      try {
        transformer = getTransformerFromFactory(ts, loaded.factory, config, program);
      } finally {
        plugin.ns += process.hrtime.bigint() - factoryStarted;
        for (const fileName of collectRequireClosure(loaded.modulePath)) {
          moduleFiles.add(fileName);
        }
      }
      if (!transformer) {
        continue;
      }
      const shouldTransformSourceFile = loaded.factoryModule.shouldTransformSourceFile;
      plugins.push(plugin);

      if (transformer.afterDeclarations) {
        transforms.afterDeclarations.push(
          wrapWithTiming(wrapWithShouldTransform(ts, transformer.afterDeclarations, shouldTransformSourceFile, program, config), plugin),
        );
      }
      if (transformer.after) {
        transforms.after.push(
          wrapWithTiming(wrapWithShouldTransform(ts, transformer.after, shouldTransformSourceFile, program, config), plugin),
        );
      }
      if (transformer.before) {
        transforms.before.push(
          wrapWithTiming(wrapWithShouldTransform(ts, transformer.before, shouldTransformSourceFile, program, config), plugin),
        );
      }
    } catch (error) {
      for (const fileName of error?.moduleFiles ?? []) {
        moduleFiles.add(fileName);
      }
      diagnostics.push(createPluginNotFoundDiagnostic(config.transform, error));
    }
  }

  return { transforms, diagnostics, plugins, moduleFiles, modulePaths };
}

function flattenIntoTransformers(transforms) {
  return [
    ...transforms.before,
    ...transforms.after,
  ];
}

function fixOrphanedParents(ts, root) {
  const walk = (parent) => {
    ts.forEachChild(parent, (child) => {
      if (!child.parent || !child.parent.getSourceFile?.()) {
        child.parent = parent;
      }
      walk(child);
    });
  };
  walk(root);
}

function wrapTransformersWithParentFix(ts, transformers) {
  const lastIndex = transformers.length - 1;
  return transformers.map((factory, index) => {
    if (index === lastIndex) {
      return factory;
    }
    return (context) => {
      const transform = factory(context);
      return (sourceFile) => {
        const result = transform(sourceFile);
        if (result !== sourceFile && ts.isSourceFile(result)) {
          fixOrphanedParents(ts, result);
        }
        return result;
      };
    };
  });
}

module.exports = {
  createTransformerList,
  collectRequireClosure,
  pluginMetrics,
  resetPluginMetrics,
  flattenIntoTransformers,
  getPluginConfigs,
  validatePluginConfigs,
  wrapTransformersWithParentFix,
};
