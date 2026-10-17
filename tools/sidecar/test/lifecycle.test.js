const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const ts = require("typescript");
const { SidecarServer } = require("../lib/session");

test("config replacements release language service documents, including validation-only requests", (t) => {
  const projectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-lifecycle-"));
  t.after(() => fs.rmSync(projectDir, { recursive: true, force: true }));
  const fileName = path.join(projectDir, "main.ts");
  const tsConfigPath = path.join(projectDir, "tsconfig.json");
  fs.writeFileSync(fileName, "export const value = 1;\n");
  const counts = { acquired: 0, released: 0, created: 0, disposed: 0 };
  const instrumented = new Proxy(ts, {
    get(target, name) {
      if (name === "createDocumentRegistry") {
        return (...args) => {
          const registry = target.createDocumentRegistry(...args);
          for (const [method, counter] of [["acquireDocumentWithKey", "acquired"], ["releaseDocumentWithKey", "released"]]) {
            const original = registry[method];
            registry[method] = (...args) => {
              counts[counter] += 1;
              return original.apply(registry, args);
            };
          }
          return registry;
        };
      }
      if (name === "createLanguageService") {
        return (...args) => {
          counts.created += 1;
          const service = target.createLanguageService(...args);
          const dispose = service.dispose;
          service.dispose = () => {
            counts.disposed += 1;
            dispose.call(service);
          };
          return service;
        };
      }
      return target[name];
    },
  });
  const server = new SidecarServer(instrumented);
  t.after(() => server.dispose());
  const request = { protocol: 1, operation: "transform", projectDir, tsConfigPath, compileFileNames: [fileName], changedFiles: [], plugins: [] };
  const writeConfig = (strict, target) => fs.writeFileSync(tsConfigPath, JSON.stringify({ compilerOptions: { noLib: true, strict, target }, files: ["main.ts"] }));
  const transform = () => {
    const response = server.handleRequest(request);
    assert.deepEqual(response.diagnostics, []);
    assert.deepEqual(response.transformed, []);
  };
  writeConfig(false, "ES2020");
  transform();
  transform();
  assert.equal(counts.created, 1, "unchanged config should reuse its service");
  assert.equal(counts.disposed, 0);
  for (const [strict, target] of [[true, "ES2020"], [false, "ES2022"], [true, "ESNext"]]) {
    writeConfig(strict, target);
    transform();
    assert.equal(counts.disposed, counts.created - 1, "each replaced service must release its registry ownership");
    assert.equal(counts.acquired - counts.released, 1, "only the current program should own the document");
  }
  writeConfig(false, "ES2019");
  const validated = server.handleRequest({ ...request, operation: "validate" });
  assert.deepEqual(validated.diagnostics, []);
  assert.equal(counts.disposed, counts.created, "validation must dispose an invalidated service without creating another");
  assert.equal(counts.acquired, counts.released);
  transform();
  server.dispose();
  server.dispose();
  assert.equal(counts.created, counts.disposed, "server disposal must release every service exactly once");
  assert.ok(counts.acquired > 0, "exercise real document registry ownership");
  assert.equal(counts.acquired, counts.released);
});
