"use strict";

const fs = require("node:fs");
const readline = require("node:readline");

const mode = process.argv[2];
const marker = process.argv[3];

if (marker && mode !== "blocked-build") fs.writeFileSync(marker, String(process.pid));

const lines = readline.createInterface({ input: process.stdin });
let callbackBuildID;
lines.on("line", (line) => {
	const request = JSON.parse(line);
	if (mode === "transform-callback" && request.id === "transform-1") {
		if (
			request.error ||
			request.result?.diagnostics?.[0]?.code !== "invalid-request" ||
			!Array.isArray(request.result?.transformed)
		) {
			process.exit(24);
			return;
		}
		process.stdout.write(
			`${JSON.stringify({
				jsonrpc: "2.0",
				id: callbackBuildID,
				result: {
					ok: true,
					files: 0,
					durationMs: 0,
					diagnostics: [],
					outputs: [],
					projects: [],
					telemetry: { scheduledProjects: 0, transformedProjects: 0, emittedProjects: 0 },
				},
			})}\n`,
		);
		return;
	}
	if (mode === "exit") process.exit(23);
	if (mode === "malformed") {
		process.stdout.write("this is not json\n");
		return;
	}
	if (mode === "invalid-envelope") {
		process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, error: null })}\n`);
		return;
	}
	if (request.method === "initialize") {
		const respond = () => process.stdout.write(
			`${JSON.stringify({
				jsonrpc: "2.0",
				id: request.id,
				result: {
					protocolVersion: 1,
					serverVersion: mode === "version-mismatch" ? "0.0.0" : "2.6.0",
					capabilities: ["build", "shutdown", "terminal-cancel", "transformer-callback"],
				},
			})}\n`,
		);
		if (mode === "delayed-initialize") setTimeout(respond, 250);
		else respond();
		return;
	}
	if (request.method === "build") {
		if (mode === "blocked-build") {
			fs.writeFileSync(marker, String(process.pid));
			return;
		}
		if (mode === "transform-callback") {
			callbackBuildID = request.id;
			process.stdout.write(
				`${JSON.stringify({ jsonrpc: "2.0", id: "transform-1", method: "transform", params: { protocol: 0 } })}\n`,
			);
			return;
		}
		process.stdout.write(
			`${JSON.stringify({
				jsonrpc: "2.0",
				id: request.id,
				result:
					mode === "malformed-result"
						? { ok: "yes" }
						: {
							ok: true,
							files: 0,
							durationMs: 0,
							diagnostics: [],
							outputs: [],
							projects: [],
							telemetry: { scheduledProjects: 0, transformedProjects: 0, emittedProjects: 0 },
						},
			})}\n`,
		);
		return;
	}
	if (request.method === "shutdown") {
		process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, result: {} })}\n`, () => process.exit(0));
	}
});
