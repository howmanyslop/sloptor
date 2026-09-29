#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const result = spawnSync(
	process.execPath,
	["--test", "test/platform-packaging.test.js", "test/release-packaging.test.js"],
	{ cwd: repoRoot, stdio: "inherit" },
);

if (result.error) throw result.error;
process.exit(result.status ?? 1);
