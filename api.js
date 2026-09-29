"use strict";

const fs = require("node:fs");
const path = require("node:path");
const readline = require("node:readline");
const { spawn } = require("node:child_process");

const pkg = require("./package.json");

const PROTOCOL_VERSION = 1;
const REQUIRED_CAPABILITIES = ["build", "shutdown", "terminal-cancel"];

function binaryPath() {
	const platforms = { win32: "windows", linux: "linux", darwin: "darwin" };
	const architectures = { x64: "amd64", arm64: "arm64" };
	const platform = platforms[process.platform];
	const architecture = architectures[process.arch];
	if (!platform || !architecture) {
		throw new SloptorClientError(
			"UNSUPPORTED_PLATFORM",
			`Sloptor does not support ${process.platform}-${process.arch}; expected Windows, Linux, or macOS on x64 or arm64`,
		);
	}
	const extension = platform === "windows" ? ".exe" : "";
	return path.join(__dirname, "bin", `sloptor-${platform}-${architecture}${extension}`);
}

class SloptorClientError extends Error {
	constructor(code, message, options) {
		super(message, options);
		this.name = "SloptorClientError";
		this.code = code;
	}
}

class BuildSession {
	#child;
	#connected = false;
	#connecting;
	#disposed = false;
	#executable;
	#executableArgs;
	#installIfMissing;
	#nextID = 1;
	#pending = new Map();
	#queue = Promise.resolve();
	#terminalError;
	#exitPromise;
	#stderr = "";
	#abortPromise;

	constructor(executable, executableArgs, installIfMissing) {
		this.#executable = executable;
		this.#executableArgs = executableArgs;
		this.#installIfMissing = installIfMissing;
	}

	async #connect() {
		await this.#resolveExecutable();

		this.#child = spawn(this.#executable, [...this.#executableArgs, "api-server"], {
			stdio: ["pipe", "pipe", "pipe"],
			windowsHide: true,
		});
		this.#exitPromise = new Promise((resolve) => {
			this.#child.once("close", (code, signal) => {
				resolve({ code, signal });
				if (!this.#terminalError && (!this.#disposed || this.#pending.size > 0)) {
					const detail = signal ? `signal ${signal}` : `exit code ${code}`;
					this.#fail(
						new SloptorClientError(
							"SERVER_EXITED",
							`Sloptor API server exited prematurely with ${detail}${this.#stderrDetail()}`,
						),
					);
				}
			});
		});
		this.#child.once("error", (cause) => {
			const code = cause && cause.code === "ENOENT" ? "EXECUTABLE_NOT_FOUND" : "SPAWN_FAILED";
			this.#fail(new SloptorClientError(code, `Failed to start Sloptor API server: ${cause.message}`, { cause }));
		});
		this.#child.stdin.on("error", (cause) => {
			this.#fail(
				new SloptorClientError("SERVER_EXITED", `Lost the Sloptor API server input stream${this.#stderrDetail()}`, {
					cause,
				}),
			);
		});
		this.#child.stderr.setEncoding("utf8");
		this.#child.stderr.on("data", (chunk) => {
			this.#stderr = (this.#stderr + chunk).slice(-8192);
		});

		const lines = readline.createInterface({ input: this.#child.stdout });
		lines.on("line", (line) => this.#receive(line));

		const handshake = await this.#request("initialize", {
			protocolVersion: PROTOCOL_VERSION,
			clientVersion: pkg.version,
		});
		this.#validateHandshake(handshake);
		this.#connected = true;
	}

	build(request) {
		if (this.#disposed) {
			return Promise.reject(new SloptorClientError("SESSION_DISPOSED", "The Sloptor build session is disposed"));
		}
		const hasProject = request && request.project !== undefined;
		const hasRoots = request && request.roots !== undefined;
		if (hasProject === hasRoots) {
			return Promise.reject(new SloptorClientError("INVALID_REQUEST", "provide project or roots, not both"));
		}
		if (hasProject && (typeof request.project !== "string" || request.project.length === 0)) {
			return Promise.reject(new SloptorClientError("INVALID_REQUEST", "project must be a non-empty path"));
		}
		if (
			hasRoots &&
			(!Array.isArray(request.roots) ||
				request.roots.length === 0 ||
				request.roots.some((root) => typeof root !== "string" || root.length === 0))
		) {
			return Promise.reject(new SloptorClientError("INVALID_REQUEST", "roots must contain one or more non-empty paths"));
		}
		const roots = hasProject ? [request.project] : request.roots;

		const operation = this.#queue.then(async () => {
			if (request.signal?.aborted) {
				throw new SloptorClientError("BUILD_CANCELLED", "The build was cancelled");
			}
			const onConnectAbort = () => void this.#abortSession();
			request.signal?.addEventListener("abort", onConnectAbort, { once: true });
			try {
				await this.#ensureConnected();
			} finally {
				request.signal?.removeEventListener("abort", onConnectAbort);
			}
			if (request.signal?.aborted) {
				await this.#abortSession();
				throw new SloptorClientError("BUILD_CANCELLED", "The build was cancelled");
			}
			const result = await this.#request("build", { roots: roots.map((root) => path.resolve(root)) }, request.signal);
			return this.#validateBuildResult(result);
		});
		this.#queue = operation.catch(() => {});
		return operation;
	}

	dispose() {
		if (this.#disposed) return this.#queue;
		this.#disposed = true;
		const graceful = this.#queue.then(async () => {
			if (!this.#child) return;
			if (this.#child.exitCode !== null || this.#terminalError) {
				await this.#exitPromise;
				return;
			}
			try {
				await this.#request("shutdown", {});
			} catch (error) {
				if (!this.#terminalError) throw error;
			} finally {
				this.#child.stdin.end();
			}
			await this.#exitPromise;
		});
		const operation = (async () => {
			let timeout;
			try {
				const finished = await Promise.race([
					graceful.then(() => true),
					new Promise((resolve) => {
						timeout = setTimeout(() => resolve(false), 2000);
					}),
				]);
				if (!finished) {
					await this.#terminate(
						new SloptorClientError("SESSION_DISPOSED", "The Sloptor build session was terminated during disposal"),
					);
				}
			} catch (error) {
				await this.#terminate(error);
				throw error;
			} finally {
				clearTimeout(timeout);
			}
		})();
		this.#queue = operation.catch(() => {});
		return operation;
	}

	[Symbol.asyncDispose]() {
		return this.dispose();
	}

	#ensureConnected() {
		if (this.#terminalError) return Promise.reject(this.#terminalError);
		if (this.#connected) return Promise.resolve();
		if (!this.#connecting) {
			this.#connecting = this.#connect().catch(async (error) => {
				await this.#terminate(error);
				throw error;
			});
		}
		return this.#connecting;
	}

	#request(method, params, signal) {
		if (this.#terminalError) return Promise.reject(this.#terminalError);
		if (signal?.aborted) {
			return this.#abortSession().then(() => {
				throw new SloptorClientError("BUILD_CANCELLED", "The build was cancelled");
			});
		}
		const id = this.#nextID++;
		return new Promise((resolve, reject) => {
			let onAbort;
			if (signal) {
				onAbort = () => void this.#abortSession();
				signal.addEventListener("abort", onAbort, { once: true });
			}
			this.#pending.set(id, {
				resolve,
				reject,
				cleanup: () => signal?.removeEventListener("abort", onAbort),
			});
			try {
				this.#send({ id, method, params });
			} catch (cause) {
				this.#pending.delete(id);
				signal?.removeEventListener("abort", onAbort);
				reject(new SloptorClientError("SERVER_EXITED", "Could not write to the Sloptor API server", { cause }));
			}
		});
	}

	#send(message) {
		this.#child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
	}

	#receive(line) {
		if (this.#terminalError) return;
		let message;
		try {
			message = JSON.parse(line);
		} catch (cause) {
			this.#protocolFailure("Sloptor API server sent malformed JSON", cause);
			return;
		}
		if (!message || message.jsonrpc !== "2.0") {
			this.#protocolFailure("Sloptor API server sent an invalid response");
			return;
		}
		if (typeof message.method === "string") {
			if (typeof message.id === "string") {
				this.#send({
					id: message.id,
					error: { code: "METHOD_NOT_FOUND", message: `Unsupported server method ${message.method}` },
				});
			}
			return;
		}
		const hasResult = "result" in message;
		const hasError = "error" in message;
		if (!Number.isSafeInteger(message.id) || hasResult === hasError) {
			this.#protocolFailure("Sloptor API server sent an invalid response");
			return;
		}
		if (
			hasError &&
			(!message.error || typeof message.error.code !== "string" || typeof message.error.message !== "string")
		) {
			this.#protocolFailure("Sloptor API server sent an invalid error response");
			return;
		}
		const pending = this.#pending.get(message.id);
		if (!pending) {
			this.#protocolFailure(`Sloptor API server responded with unknown request id ${message.id}`);
			return;
		}
		this.#pending.delete(message.id);
		pending.cleanup();
		if (message.error) {
			pending.reject(new SloptorClientError(message.error.code || "SERVER_ERROR", message.error.message || "Sloptor API server error"));
		} else {
			pending.resolve(message.result);
		}
	}

	#validateHandshake(handshake) {
		if (!handshake || handshake.protocolVersion !== PROTOCOL_VERSION) {
			throw new SloptorClientError(
				"PROTOCOL_VERSION_MISMATCH",
				`Sloptor API protocol mismatch: expected ${PROTOCOL_VERSION}, received ${handshake?.protocolVersion}`,
			);
		}
		if (handshake.serverVersion !== pkg.version) {
			throw new SloptorClientError(
				"VERSION_MISMATCH",
				`Sloptor package version ${pkg.version} does not match server version ${handshake.serverVersion}`,
			);
		}
		for (const capability of REQUIRED_CAPABILITIES) {
			if (!Array.isArray(handshake.capabilities) || !handshake.capabilities.includes(capability)) {
				throw new SloptorClientError("CAPABILITY_MISMATCH", `Sloptor API server is missing capability ${capability}`);
			}
		}
	}

	#protocolFailure(message, cause) {
		void this.#terminate(new SloptorClientError("PROTOCOL_ERROR", message, { cause }));
	}

	#fail(error) {
		if (this.#terminalError) return;
		this.#terminalError = error;
		for (const pending of this.#pending.values()) {
			pending.cleanup();
			pending.reject(error);
		}
		this.#pending.clear();
	}

	#stderrDetail() {
		const detail = this.#stderr.trim();
		return detail ? `: ${detail}` : "";
	}

	#abortSession() {
		if (this.#abortPromise) return this.#abortPromise;
		this.#abortPromise = (async () => {
			if (this.#terminalError) return;
			this.#terminalError = new SloptorClientError(
				"SESSION_TERMINAL",
				"The Sloptor build session terminated after cancellation",
			);
			if (this.#child?.exitCode === null) this.#child.kill();
			if (this.#exitPromise) await this.#exitPromise;
			const cancellation = new SloptorClientError("BUILD_CANCELLED", "The build was cancelled");
			for (const pending of this.#pending.values()) {
				pending.cleanup();
				pending.reject(cancellation);
			}
			this.#pending.clear();
		})();
		return this.#abortPromise;
	}

	async #terminate(error) {
		this.#fail(error);
		if (this.#child?.exitCode === null) this.#child.kill();
		if (this.#exitPromise) await this.#exitPromise;
	}

	async #resolveExecutable() {
		if (fs.existsSync(this.#executable)) return;
		if (this.#installIfMissing) {
			try {
				const { install } = require("./scripts/install.js");
				this.#executable = await install({ quiet: true });
			} catch (cause) {
				const detail = cause instanceof Error ? cause.message : String(cause);
				throw new SloptorClientError(
					"EXECUTABLE_NOT_FOUND",
					`Could not install the Sloptor executable: ${detail}`,
					{ cause },
				);
			}
		}
		if (!fs.existsSync(this.#executable)) {
			throw new SloptorClientError(
				"EXECUTABLE_NOT_FOUND",
				`Sloptor executable was not found at ${this.#executable}`,
			);
		}
	}

	async #validateBuildResult(result) {
		const validDiagnostic = (diagnostic) =>
			diagnostic !== null &&
			typeof diagnostic === "object" &&
			typeof diagnostic.file === "string" &&
			Number.isSafeInteger(diagnostic.line) &&
			diagnostic.line >= 0 &&
			Number.isSafeInteger(diagnostic.col) &&
			diagnostic.col >= 0 &&
			(diagnostic.code === undefined || typeof diagnostic.code === "string") &&
			(diagnostic.severity === "error" || diagnostic.severity === "warning") &&
			typeof diagnostic.message === "string";
		const validCounts = (counts) =>
			counts !== null &&
			typeof counts === "object" &&
			Object.values(counts).every((value) => Number.isSafeInteger(value) && value >= 0);
		const validStages = (stages) =>
			stages !== null &&
			typeof stages === "object" &&
			Object.values(stages).every((value) => Number.isSafeInteger(value) && value >= 0);
		const validProject = (project) =>
			project !== null &&
			typeof project === "object" &&
			typeof project.config === "string" &&
			["success", "no-change", "blocked", "failed"].includes(project.status) &&
			Array.isArray(project.blockers) &&
			project.blockers.every((blocker) => typeof blocker === "string") &&
			Array.isArray(project.diagnostics) &&
			project.diagnostics.every(validDiagnostic) &&
			Array.isArray(project.outputs) &&
			project.outputs.every((output) => typeof output === "string") &&
			Number.isSafeInteger(project.outputCount) &&
			project.outputCount >= 0 &&
			project.timings !== null &&
			typeof project.timings === "object" &&
			Number.isSafeInteger(project.timings.durationMs) &&
			project.timings.durationMs >= 0 &&
			validStages(project.timings.stages) &&
			validCounts(project.timings.counts);
		const validTelemetry = (telemetry) =>
			telemetry !== null &&
			typeof telemetry === "object" &&
			Number.isSafeInteger(telemetry.scheduledProjects) &&
			telemetry.scheduledProjects >= 0 &&
			Number.isSafeInteger(telemetry.transformedProjects) &&
			telemetry.transformedProjects >= 0 &&
			Number.isSafeInteger(telemetry.emittedProjects) &&
			telemetry.emittedProjects >= 0;
		const valid =
			result !== null &&
			typeof result === "object" &&
			typeof result.ok === "boolean" &&
			Number.isSafeInteger(result.files) &&
			result.files >= 0 &&
			Number.isSafeInteger(result.durationMs) &&
			result.durationMs >= 0 &&
			Array.isArray(result.diagnostics) &&
			result.diagnostics.every(validDiagnostic) &&
			Array.isArray(result.outputs) &&
			result.outputs.every((output) => typeof output === "string") &&
			Array.isArray(result.projects) &&
			result.projects.every(validProject) &&
			validTelemetry(result.telemetry);
		if (!valid) {
			const error = new SloptorClientError("PROTOCOL_ERROR", "Sloptor API server sent an invalid build result");
			await this.#terminate(error);
			throw error;
		}
		return result;
	}
}

function createBuildSession(options = {}) {
	const executableArgs = options.executableArgs || [];
	if (!Array.isArray(executableArgs) || executableArgs.some((value) => typeof value !== "string")) {
		throw new SloptorClientError("INVALID_REQUEST", "executableArgs must contain only strings");
	}
	return new BuildSession(options.executable || binaryPath(), [...executableArgs], options.executable === undefined);
}

module.exports = { createBuildSession, SloptorClientError };
