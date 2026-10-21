// Native transport stand-in only; the API client runs its real transformer host.
const fs = require("node:fs");
const path = require("node:path");
const readline = require("node:readline");
const { version } = require("../../../package.json");
const directory = process.argv[2];
let buildID;
let sequence = 0;
const send = (id, result) => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id, result })}\n`);
readline.createInterface({ input: process.stdin }).on("line", (line) => {
  const request = JSON.parse(line);
  if (request.method === "initialize") {
    send(request.id, { protocolVersion: 1, serverVersion: version,
      capabilities: ["build", "project-selection", "shutdown", "terminal-cancel", "transformer-callback"] });
  } else if (request.method === "build") {
    buildID = request.id;
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: "transform", method: "transform",
      params: JSON.parse(fs.readFileSync(path.join(directory, "request.json"), "utf8")) })}\n`);
  } else if (request.id === "transform") {
    if (request.error) throw new Error(JSON.stringify(request.error));
    fs.writeFileSync(path.join(directory, `response-${sequence++}.json`), JSON.stringify(request.result));
    send(buildID, { ok: true, files: 0, durationMs: 0, diagnostics: [], outputs: [], projects: [],
      telemetry: { selectedProjects: 0, satisfiedProjects: 0, scheduledProjects: 0, transformedProjects: 0, emittedProjects: 0 } });
  } else if (request.method === "shutdown") {
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, result: {} })}\n`, () => process.exit(0));
  }
});
