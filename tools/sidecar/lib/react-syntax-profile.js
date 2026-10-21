const fs = require("node:fs");
const path = require("node:path");

// Source-reviewed original-input factory. These are expected release digests,
// never hashes learned from whatever happens to be installed in the project.
const ENTRY = { name: "index.cjs", sha256: "28b471f2284cd8033284f8a399daefa2ecd389cdffcfdd9af4ff89b19bb86da7" };
const CHUNK = { name: "apply-memoization-a22YdxsT.cjs", sha256: "06c6eb5839656dfd05f358972c0da1c51d3defad4a274f27e81a64b47b71a0f0" };
// Separate Oxc release profile; the old JS factory and its options stay pinned.
const OXC_ENTRY = { name: "index.cjs", sha256: "98e9c4e16ca657088909cf2923ac438a8068bd190abdf3a203c2dad829c13e45" };
const OXC_CHUNK = { name: "apply-memoization-o2f7rwQZ.cjs", sha256: "5e86cb00eaaf9548f0440f15e3488c175ebef9c985e8b829a9e07de02c847717" };
const OXC_RUNTIME = { name: "rolldown-runtime-VH7oDXx4.cjs", sha256: "3728f8d3a4904c0fdd49ef3abeb40e128739a6a3f67c5c05994053e2ef6f14ee" };
const OXC_BINDING = { name: "anime-rush-react-compiler-oxc.win32-x64.node", sha256: "ff336ce607714be9dd27ce38861c3cfc5dd753c670a6adc793eb85ff28a33220" };
const OXC_UPSTREAM = { name: "upstream-source.json", sha256: "904cafce9072092f116e9fad4c8a1d57e65995beebc411eb9e04777998c7b048" };
// These literal imports are still loaded by the Oxc bundle. Pin their entries
// without traversing a dependency store or claiming a transitive inventory.
const OXC_IMPORTS = [
  { request: "@babel/core", name: "index.js", sha256: "dba868255b447218cd421b30fb5289c118f265817088e7106053cf962057dd26" },
  { request: "@babel/types", name: "index.js", sha256: "21af18add84dbb64a624552e04abad89a40affacf701776d4a662aedef6accfd" },
  { request: "babel-plugin-react-compiler", name: "index.cjs", sha256: "5a4ef491cf7486c647dbc67d46e5ac3819f04a0843298a374ab638683daab33f" },
];
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

function oxcArtifacts(entryPath, chunkPath) {
  // Match the pinned backend's compilerRepoRoot lookup without loading it.
  let directory = path.dirname(chunkPath);
  while (!fs.existsSync(path.join(directory, "pnpm-workspace.yaml"))) {
    const parent = path.dirname(directory);
    if (parent === directory) throw new Error("Unsupported React compiler workspace");
    directory = parent;
  }
  const nativeRoot = path.join(directory, "tools", "react-compiler-oxc");
  const runtime = pin(path.join(path.dirname(entryPath), OXC_RUNTIME.name), OXC_RUNTIME);
  if (pin(require.resolve(`./${OXC_RUNTIME.name}`, { paths: [path.dirname(chunkPath)] }), OXC_RUNTIME).path !== runtime.path) {
    throw new Error("Unsupported React compiler runtime path");
  }
  return {
    runtime,
    binding: pin(path.join(nativeRoot, "dist", OXC_BINDING.name), OXC_BINDING),
    upstream: pin(path.join(nativeRoot, OXC_UPSTREAM.name), OXC_UPSTREAM),
    imports: OXC_IMPORTS.map(expected => pin(require.resolve(expected.request, { paths: [path.dirname(chunkPath)] }), expected)),
  };
}

function resolveReactSyntaxProfile(projectDir, tsModulePath, configs) {
  try {
    if (!Array.isArray(configs) || configs.length !== 1) return;
    const transform = configs[0]?.transform;
    if (typeof transform !== "string" ||
        (transform !== "anime-rush-react-compiler-applier" && !path.isAbsolute(transform))) return;
    const oxc = configs[0].backend === "oxc";
    if (oxc && (process.platform !== "win32" || process.arch !== "x64")) return;
    const entry = pin(require.resolve(transform, { paths: [projectDir] }), oxc ? OXC_ENTRY : ENTRY);
    const expectedChunk = oxc ? OXC_CHUNK : CHUNK;
    const chunk = pin(require.resolve(`./${expectedChunk.name}`, { paths: [path.dirname(entry.path)] }), expectedChunk);
    const module = pin(tsModulePath, WRAPPER);
    const implementation = pin(require.resolve("@typescript/old", { paths: [path.dirname(module.path)] }), TYPESCRIPT);
    // The selector checks the complete raw config against these exact shapes,
    // every digest, the shared TS object and root/reference ownership before
    // requiring a plugin. This resolver performs no module evaluation.
    return freeze({
      kind: oxc ? "react-original-source-oxc-v1" : "react-original-source-v1", entry, chunk,
      ...(oxc ? oxcArtifacts(entry.path, chunk.path) : {}),
      typescript: { version: "6.0.3", module, implementation },
      chains: [
        [{ after: true, transform, include: ["**", "!**/*.test.tsx"], ...(oxc ? { backend: "oxc" } : {}) }],
        [{ after: true, transform, include: ["**", "!**/use-sync-external-store-with-selector.tsx", "!**/*.test.tsx"], ...(oxc ? { backend: "oxc" } : {}) }],
      ],
    });
  } catch {
    return undefined;
  }
}

module.exports = { resolveReactSyntaxProfile };
