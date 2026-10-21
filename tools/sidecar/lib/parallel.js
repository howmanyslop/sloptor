const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { isDeepStrictEqual } = require("node:util");

function packageRoot(entry) {
  // Real Node package entries (including pnpm's physical store paths) have
  // an unambiguous installation root after the last node_modules segment.
  // Do not accidentally attest only a nested dist package's manifest.
  const segments = path.resolve(entry).split(path.sep);
  const modules = segments.map((segment) => process.platform === "win32" ? segment.toLowerCase() : segment).lastIndexOf("node_modules");
  if (modules >= 0) {
    const end = modules + (segments[modules + 1]?.startsWith("@") ? 3 : 2);
    const root = segments.slice(0, end).join(path.sep);
    const metadata = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
    if (typeof metadata.name !== "string" || metadata.name.length === 0) {
      throw new Error("resolved module has no named package root");
    }
    return root;
  }
  for (let dir = path.dirname(entry); ; dir = path.dirname(dir)) {
    // A dist/cjs/package.json containing only { type: "commonjs" } is
    // not the package root. Include the owning package's manifest and code.
    // Never cross node_modules into another package if ownership is unknown.
    if (path.basename(dir) === "node_modules") {
      throw new Error("resolved module has no named package root");
    }
    const manifest = path.join(dir, "package.json");
    if (fs.statSync(manifest, { throwIfNoEntry: false })?.isFile()) {
      const metadata = JSON.parse(fs.readFileSync(manifest, "utf8"));
      if (typeof metadata.name === "string" && metadata.name.length > 0) {
        return dir;
      }
    }
    if (path.dirname(dir) === dir) {
      throw new Error("resolved module has no named package root");
    }
  }
}

// A portable content identity, not an authentication scheme. Hash every
// regular package file (including code and package.json), excluding nested
// node_modules. Relative names use '/', sorted by JS code-unit order. The
// domain, each relative name and decimal byte length are NUL terminated.
// Symlinks/special files and unusually large packages select serial.
function packageDigest(entry) {
  const root = packageRoot(fs.realpathSync(entry));
  const files = [];
  let total = 0;
  let entries = 0;
  function visit(dir) {
    for (const child of fs.readdirSync(dir, { withFileTypes: true })) {
      if (++entries > 8192) {
        throw new Error("package entry limit");
      }
      if (child.name === "node_modules") {
        continue;
      }
      const name = path.join(dir, child.name);
      if (child.isDirectory()) {
        visit(name);
      } else if (child.isFile()) {
        const size = fs.statSync(name).size;
        total += size;
        if (total > 64 * 1024 * 1024 || files.length >= 4096) {
          throw new Error("package content limit");
        }
        files.push(path.relative(root, name).split(path.sep).join("/"));
      } else {
        throw new Error("package contains a link or special file");
      }
    }
  }
  visit(root);
  const hash = crypto.createHash("sha256").update("rotor-package-v1\0");
  for (const name of files.sort()) {
    const bytes = fs.readFileSync(path.join(root, name));
    hash.update(`${name}\0${bytes.length}\0`).update(bytes);
  }
  return hash.digest("hex");
}

function matchesPackage(entry, expected) {
  return typeof expected === "string" && /^[a-f0-9]{64}$/.test(expected) && packageDigest(entry) === expected;
}

function preflightParallelTransformers(ts, tsModulePath, request) {
  const profile = request.parallelAttestation;
  if (!profile || profile.workers !== 2 || profile.batchIndependent !== true ||
      profile.typescript !== ts.version || !tsModulePath ||
      request.rootFileNames !== undefined || !Array.isArray(request.plugins) ||
      !Array.isArray(profile.plugins) || profile.plugins.length !== request.plugins.length ||
      profile.plugins.length === 0) {
    return false;
  }
  const entries = [];
  for (let index = 0; index < profile.plugins.length; index++) {
    const attested = profile.plugins[index];
    const plugin = request.plugins[index];
    if (!plugin || typeof plugin.transform !== "string" ||
        !isDeepStrictEqual(JSON.parse(attested.config), plugin) ||
        ![undefined, "program", "checker", "compilerOptions", "config", "raw"].includes(plugin.type)) {
      return false;
    }
    // Keep normal, direct plugin resolution. In particular, do not use a
    // sandbox copy or infer admission from a familiar package name/version.
    const entry = require.resolve(plugin.transform, { paths: [request.projectDir] });
    if (!matchesPackage(entry, attested.sha256)) {
      return false;
    }
    entries.push(entry);
  }
  for (const dependency of profile.dependencies ?? []) {
    if (!Number.isInteger(dependency.from) || dependency.from < 0 || dependency.from >= entries.length ||
        typeof dependency.specifier !== "string" || dependency.specifier.length === 0) {
      return false;
    }
    const entry = require.resolve(dependency.specifier, { paths: [path.dirname(entries[dependency.from])] });
    if (!matchesPackage(entry, dependency.sha256)) {
      return false;
    }
  }
  // Resolution and package bytes only: never evaluate plugins or their TS
  // peers here. Export and same-instance checks remain after Program creation
  // in the ordinary transform path. Bind the already-loaded engine's version,
  // never a wrapper's package metadata or a second evaluation of TypeScript.
  return true;
}

module.exports = { packageDigest, preflightParallelTransformers };
