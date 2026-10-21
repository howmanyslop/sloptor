const fs = require("node:fs");
const path = require("node:path");

// Source-reviewed original-input factory. These are expected release digests,
// never hashes learned from whatever happens to be installed in the project.
const ENTRY = { name: "index.cjs", sha256: "28b471f2284cd8033284f8a399daefa2ecd389cdffcfdd9af4ff89b19bb86da7" };
const CHUNK = { name: "apply-memoization-a22YdxsT.cjs", sha256: "06c6eb5839656dfd05f358972c0da1c51d3defad4a274f27e81a64b47b71a0f0" };
const WRAPPER = { name: "typescript.js", sha256: "d3f3cd2b04b7f466f4484df921b744223f7bd1f3e353ec9110bdf52695b983d5" };
const TYPESCRIPT = { name: "typescript.js", sha256: "569177652966bd528c319171c7dd22860dbf72bde116cbc4f644f1d02bb12e39" };

function freeze(value) {
  if (value && typeof value === "object") {
    Object.values(value).forEach(freeze);
    Object.freeze(value);
  }
  return value;
}

function pin(fileName, expected) {
  const physical = fs.realpathSync.native?.(fileName) ?? fs.realpathSync(fileName);
  if (path.basename(physical) !== expected.name) throw new Error("Unsupported React syntax profile path");
  return { path: physical, sha256: expected.sha256 };
}

function resolveReactSyntaxProfile(projectDir, tsModulePath, configs) {
  try {
    if (!Array.isArray(configs) || configs.length !== 1) return;
    const transform = configs[0]?.transform;
    if (typeof transform !== "string" ||
        (transform !== "anime-rush-react-compiler-applier" && !path.isAbsolute(transform))) return;
    const entry = pin(require.resolve(transform, { paths: [projectDir] }), ENTRY);
    const chunk = pin(require.resolve(`./${CHUNK.name}`, { paths: [path.dirname(entry.path)] }), CHUNK);
    const module = pin(tsModulePath, WRAPPER);
    const implementation = pin(require.resolve("@typescript/old", { paths: [path.dirname(module.path)] }), TYPESCRIPT);
    // The selector checks the complete raw config against these exact shapes,
    // every digest, the shared TS object and root/reference ownership before
    // requiring a plugin. This resolver performs no module evaluation.
    return freeze({
      kind: "react-original-source-v1", entry, chunk,
      typescript: { version: "6.0.3", module, implementation },
      chains: [
        [{ after: true, transform, include: ["**", "!**/*.test.tsx"] }],
        [{ after: true, transform, include: ["**", "!**/use-sync-external-store-with-selector.tsx", "!**/*.test.tsx"] }],
      ],
    });
  } catch {
    return undefined;
  }
}

module.exports = { resolveReactSyntaxProfile };
