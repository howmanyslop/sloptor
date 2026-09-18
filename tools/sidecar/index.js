const fs = require("node:fs");
const readline = require("node:readline");
const { createInternalDiagnostic, createRequestDiagnostic } = require("./lib/diagnostics");
const {
  collectRequireClosure,
  createTransformerList,
  flattenIntoTransformers,
  getPluginConfigs,
  resetPluginMetrics,
  validatePluginConfigs,
} = require("./lib/plugins");
const { SidecarProjectSession, SidecarServer } = require("./lib/session");

// resolveTypeScript loads the same `typescript` module instance the project's
// plugins resolve, so factory nodes and the transform context share one
// implementation (upstream roblox-ts guarantees this by construction). The
// sidecar's own install is only a fallback for projects without typescript.
function resolveTypeScriptPath(projectDir) {
  const paths = [];
  if (typeof projectDir === "string" && projectDir.length > 0) {
    paths.push(projectDir);
  }
  paths.push(__dirname);
  const modulePath = require.resolve("typescript", { paths });
  return fs.realpathSync.native?.(modulePath) ?? fs.realpathSync(modulePath);
}

function resolveTypeScript(projectDir) {
  return require(resolveTypeScriptPath(projectDir));
}
resolveTypeScript.modulePathFor = resolveTypeScriptPath;

function transformSourceFiles(tsApi, program, sourceFiles, transforms) {
  const session = new SidecarProjectSession(tsApi, process.cwd(), process.cwd());
  try {
    const result = session.transformSourceFiles(program, sourceFiles, transforms);
    delete result.resultHandle;
    return result;
  } finally {
    session.dispose();
  }
}

function serveStdio(options = {}) {
  const input = options.input ?? process.stdin;
  const output = options.output ?? process.stdout;
  const server = options.server ?? new SidecarServer(options.ts ?? resolveTypeScript);
  const lineReader = readline.createInterface({
    input,
    crlfDelay: Infinity,
  });

  lineReader.on("line", (line) => {
    if (!line.trim()) {
      return;
    }

    let response;
    try {
      const request = JSON.parse(line);
      response = server.handleRequest(request);
    } catch (error) {
      response = {
        diagnostics: [
          error instanceof SyntaxError
            ? createRequestDiagnostic(`invalid JSON request: ${error.message}`)
            : createInternalDiagnostic(error),
        ],
        transformed: [],
      };
    }

    output.write(`${JSON.stringify(response)}\n`);
  });
  lineReader.once("close", () => server.close());

  return lineReader;
}

module.exports = {
  SidecarProjectSession,
  SidecarServer,
  collectRequireClosure,
  createTransformerList,
  flattenIntoTransformers,
  getPluginConfigs,
  resetPluginMetrics,
  validatePluginConfigs,
  resolveTypeScript,
  resolveTypeScriptPath,
  serveStdio,
  transformSourceFiles,
};
