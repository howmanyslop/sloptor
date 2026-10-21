import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import path from "node:path";

const require = createRequire(import.meta.url);
const packagePath = require.resolve("@isentinel/eslint-config/package.json");
const cliPath = path.resolve(path.dirname(packagePath), require(packagePath).bin["isentinel-lint"]);
const result = spawnSync(
	process.execPath,
	[
		cliPath,
		"--concurrency=off",
		"--eslint-args=--config eslint.config.ts",
		"--oxlint-args=--config oxlint.config.ts --disable-nested-config",
		...process.argv.slice(2),
	],
	{
		env: { ...process.env, NODE_OPTIONS: `${process.env.NODE_OPTIONS ?? ""} --max-old-space-size=6144` },
		stdio: "inherit",
	},
);

if (result.error) throw result.error;
if (result.signal) process.kill(process.pid, result.signal);
process.exitCode = result.status ?? 1;
