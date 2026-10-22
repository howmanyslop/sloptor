const { createRequire } = require("node:module");
const path = require("node:path");

// NODE_OPTIONS keeps spaces and escaped quotes inside double-quoted arguments.
const nodeOptions = process.env.NODE_OPTIONS?.match(/(?:[^\s"]+|"(?:\\.|[^"\\])*")+/g) ?? [];
const packageMapActive = [...process.execArgv, ...nodeOptions]
  .some((arg) => arg.replaceAll('"', "").split("=")[0].replaceAll("_", "-") === "--experimental-package-map");

// Package maps ignore require.resolve's paths option, even when it succeeds.
// Resolve bare packages from each search path and its ancestors to match
// node_modules order; relative and absolute requests keep normal resolution.
function resolveFromPaths(request, paths) {
  const packageMapLookup = packageMapActive && paths.length > 0 && !path.isAbsolute(request) && !request.startsWith(".");
  let originalError;
  try {
    const resolved = require.resolve(request, { paths });
    if (!packageMapLookup) {
      return resolved;
    }
  } catch (error) {
    if (!packageMapLookup) {
      throw error;
    }
    originalError = error;
  }
  for (const start of paths) {
    for (let dir = path.resolve(start); ; dir = path.dirname(dir)) {
      try {
        return createRequire(path.join(dir, "noop.js")).resolve(request);
      } catch (error) {
        originalError ??= error;
      }
      if (path.dirname(dir) === dir) {
        break;
      }
    }
  }
  throw originalError;
}

module.exports = { resolveFromPaths };
