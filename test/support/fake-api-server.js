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
					telemetry: {
						selectedProjects: 0,
						satisfiedProjects: 0,
						scheduledProjects: 0,
						transformedProjects: 0,
						emittedProjects: 0,
					},
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
					capabilities: ["build", "project-selection", "shutdown", "terminal-cancel", "transformer-callback"],
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
		if (mode === "missing-callback-id" || mode === "non-string-callback-id") {
			const callback = { jsonrpc: "2.0", method: "transform", params: { protocol: 1 } };
			if (mode === "non-string-callback-id") callback.id = 1;
			process.stdout.write(`${JSON.stringify(callback)}\n`);
			return;
		}
		process.stdout.write(
			`${JSON.stringify({
				jsonrpc: "2.0",
				id: request.id,
				result:
					mode === "malformed-result"
						? { ok: "yes" }
						: mode === "missing-timing-fields"
							? {
								ok: true,
								files: 0,
								durationMs: 0,
								diagnostics: [],
								outputs: [],
								projects: [
									{
										config: "fixture/tsconfig.json",
										status: "success",
										blockers: [],
										diagnostics: [],
										outputs: [],
										outputCount: 0,
										timings: { durationMs: 0, stages: {}, counts: {} },
									},
								],
								telemetry: {
									selectedProjects: 0,
									satisfiedProjects: 0,
									scheduledProjects: 0,
									transformedProjects: 0,
									emittedProjects: 0,
								},
							}
							: {
							ok: true,
							files: 0,
							durationMs: 0,
							diagnostics: [],
							outputs: [],
							projects: [],
							telemetry: {
								selectedProjects: 0,
								satisfiedProjects: 0,
								scheduledProjects: 0,
								transformedProjects: 0,
								emittedProjects: 0,
							},
						},
			})}\n`,
		);
		return;
	}
	if (request.method === "shutdown") {
		process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, result: {} })}\n`, () => process.exit(0));
	}
});
