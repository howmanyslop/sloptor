"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const { createBuildSession } = require("..");

const fakeServer = path.join(__dirname, "support", "fake-api-server.js");

function temporaryPath(name) {
	const directory = fs.mkdtempSync(path.join(os.tmpdir(), "sloptor-api-lifecycle-"));
	return { directory, file: path.join(directory, name) };
}

function fakeSession(mode, marker) {
	return createBuildSession({
		executable: process.execPath,
		executableArgs: [fakeServer, mode, marker],
	});
}

function processExists(pid) {
	try {
		process.kill(pid, 0);
		return true;
	} catch {
		return false;
	}
}

test("session startup is lazy and disposing an unused session starts no process", async () => {
	const marker = temporaryPath("started");
	const session = fakeSession("success", marker.file);
	assert.equal(fs.existsSync(marker.file), false);
	await session.dispose();
	assert.equal(fs.existsSync(marker.file), false);
	fs.rmSync(marker.directory, { force: true, recursive: true });
});

test("missing executables produce a stable client error", async () => {
	const missing = path.join(os.tmpdir(), `missing-sloptor-${process.pid}`, "sloptor");
	const session = createBuildSession({ executable: missing });
	await assert.rejects(session.build({ project: __dirname }), {
		code: "EXECUTABLE_NOT_FOUND",
	});
	await session.dispose();
});

for (const [mode, code] of [
	["version-mismatch", "VERSION_MISMATCH"],
	["malformed", "PROTOCOL_ERROR"],
	["invalid-envelope", "PROTOCOL_ERROR"],
	["exit", "SERVER_EXITED"],
]) {
	test(`${mode} startup failures produce ${code}`, async () => {
		const marker = temporaryPath("pid");
		const session = fakeSession(mode, marker.file);
		await assert.rejects(session.build({ project: __dirname }), { code });
		await session.dispose();
		fs.rmSync(marker.directory, { force: true, recursive: true });
	});
}

test("malformed build results terminate the session", async () => {
	const marker = temporaryPath("pid");
	const session = fakeSession("malformed-result", marker.file);
	await assert.rejects(session.build({ project: __dirname }), { code: "PROTOCOL_ERROR" });
	await session.dispose();
	fs.rmSync(marker.directory, { force: true, recursive: true });
});

test("transformer callbacks run in the API client process", async () => {
	const marker = temporaryPath("pid");
	const session = fakeSession("transform-callback", marker.file);
	try {
		const result = await session.build({ project: __dirname });
		assert.equal(result.ok, true);
	} finally {
		await session.dispose();
		fs.rmSync(marker.directory, { force: true, recursive: true });
	}
});

test("selected and satisfied ownership sets must be provided together", async () => {
	const marker = temporaryPath("pid");
	const session = fakeSession("success", marker.file);
	await assert.rejects(session.build({ project: __dirname, selected: [__filename] }), {
		code: "INVALID_REQUEST",
		message: /selected and satisfied together/,
	});
	await assert.rejects(session.build({ project: __dirname, selected: [], satisfied: [""] }), {
		code: "INVALID_REQUEST",
		message: /non-empty paths/,
	});
	await session.dispose();
	fs.rmSync(marker.directory, { force: true, recursive: true });
});

test("disposal does not suppress an in-flight server exit", async () => {
	const marker = temporaryPath("pid");
	const session = fakeSession("exit", marker.file);
	const build = session.build({ project: __dirname });
	const disposal = session.dispose();
	await assert.rejects(build, { code: "SERVER_EXITED" });
	await disposal;
	fs.rmSync(marker.directory, { force: true, recursive: true });
});

test("abort during the handshake terminates the child", async () => {
	const marker = temporaryPath("pid");
	const session = fakeSession("delayed-initialize", marker.file);
	const controller = new AbortController();
	const build = session.build({ project: __dirname, signal: controller.signal });

	while (!fs.existsSync(marker.file)) {
		await new Promise((resolve) => setTimeout(resolve, 10));
	}
	const pid = Number(fs.readFileSync(marker.file, "utf8"));
	controller.abort();

	await assert.rejects(build, { code: "BUILD_CANCELLED" });
	assert.equal(processExists(pid), false);
	await session.dispose();
	fs.rmSync(marker.directory, { force: true, recursive: true });
});

test("aborting an active request terminates the child and the session", async () => {
	const marker = temporaryPath("pid");
	const session = fakeSession("blocked-build", marker.file);
	const controller = new AbortController();
	const build = session.build({ project: __dirname, signal: controller.signal });

	while (!fs.existsSync(marker.file)) {
		await new Promise((resolve) => setTimeout(resolve, 10));
	}
	const pid = Number(fs.readFileSync(marker.file, "utf8"));
	controller.abort();

	await assert.rejects(build, { code: "BUILD_CANCELLED" });
	assert.equal(processExists(pid), false);
	await assert.rejects(session.build({ project: __dirname }), { code: "SESSION_TERMINAL" });
	await session.dispose();
	fs.rmSync(marker.directory, { force: true, recursive: true });
});

test("disposal waits for the owned child to exit", async () => {
	const marker = temporaryPath("pid");
	const session = fakeSession("success", marker.file);
	await session.build({ project: __dirname });
	const pid = Number(fs.readFileSync(marker.file, "utf8"));
	assert.equal(processExists(pid), true);
	await session.dispose();
	assert.equal(processExists(pid), false);
	fs.rmSync(marker.directory, { force: true, recursive: true });
});
