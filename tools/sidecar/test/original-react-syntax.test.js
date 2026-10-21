const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const test = require("node:test");

// Deliberately opt-in. A reviewed manifest pins the real React/TS installations
// and an owned fixture/output directory; ordinary sidecar tests do not load F.
const manifestPath = process.env.SLOPTOR_SYNTAX_PROFILE_MANIFEST;
test("finite real SidecarServer original-React discriminator", {
  skip: !manifestPath,
  timeout: 180000,
}, () => {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  fs.mkdirSync(manifest.outputDir, { recursive: true });
  const env = { ...process.env, ANIME_RUSH_APPLIER_CACHE: "off", NODE_DISABLE_COMPILE_CACHE: "1" };
  delete env.ANIME_RUSH_APPLIER_TIMINGS;
  delete env.NODE_COMPILE_CACHE;
  const arms = {};
  for (const arm of ["baseline", "candidate"]) {
    const result = spawnSync(process.execPath, [
      path.join(__dirname, "original-react-syntax-worker.js"), manifestPath, arm,
    ], { env, encoding: "utf8", timeout: 75000, maxBuffer: 8 * 1024 * 1024, windowsHide: true });
    // Keep raw failures/stacks before asserting. No stack/path/message
    // normalization: any host-frame discrepancy needs explicit adjudication.
    fs.writeFileSync(path.join(manifest.outputDir, `${arm}.stdout.txt`), result.stdout ?? "");
    fs.writeFileSync(path.join(manifest.outputDir, `${arm}.stderr.txt`), result.stderr ?? "");
    assert.ifError(result.error);
    assert.equal(result.status, 0, `${arm}: ${result.stderr}`);
    arms[arm] = JSON.parse(fs.readFileSync(path.join(manifest.outputDir, `${arm}.json`), "utf8"));
  }
  assert.deepEqual(arms.candidate.records.map((record) => [record.name, record.response]),
    arms.baseline.records.map((record) => [record.name, record.response]));
  assert.deepEqual(arms.candidate.factoryInputs, arms.baseline.factoryInputs,
    "actual factory receives identical text, filenames, config, parser metadata, cwd and argv");
  assert(arms.candidate.syntaxRequests > 0);
  assert(arms.candidate.genericRequests > 0);
  assert.equal(arms.baseline.syntaxRequests, 0);
  assert(arms.candidate.positiveChanges > 0, "real memoization must be observed");
});
