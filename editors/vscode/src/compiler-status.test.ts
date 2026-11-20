import { describe, it } from "vitest";

import { getCompilerStatus, pickCompilerFolder } from "./compiler-status";

const GAME = { name: "game", uri: { scheme: "file" } };
const TOOLS = { name: "tools", uri: { scheme: "file" } };
const REMOTE = { name: "remote", uri: { scheme: "vscode-remote" } };

describe("pickCompilerFolder", () => {
	it("should pick the active file's folder", ({ expect }) => {
		expect.assertions(1);

		expect(pickCompilerFolder([GAME, TOOLS], { folder: TOOLS })).toBe(TOOLS);
	});

	it("should pick nothing for an active file outside every folder", ({ expect }) => {
		expect.assertions(1);

		expect(pickCompilerFolder([GAME], {})).toBeUndefined();
	});

	it("should pick the only folder without an active editor", ({ expect }) => {
		expect.assertions(3);

		expect(pickCompilerFolder([GAME])).toBe(GAME);
		expect(pickCompilerFolder([GAME, TOOLS])).toBeUndefined();
		expect(pickCompilerFolder<typeof GAME>([])).toBeUndefined();
	});

	it("should never pick a folder that is not on this machine", ({ expect }) => {
		expect.assertions(2);

		expect(pickCompilerFolder([REMOTE], { folder: REMOTE })).toBeUndefined();
		expect(pickCompilerFolder([REMOTE])).toBeUndefined();
	});
});

describe("getCompilerStatus", () => {
	const visible = { folder: GAME, isEnabled: true, isRunning: false, languageId: "typescript" };

	it("should offer to start or stop the compiler in the folder", ({ expect }) => {
		expect.assertions(2);

		expect(getCompilerStatus(visible)).toStrictEqual({
			text: "$(debug-start) sloptor",
			tooltip: "Start the sloptor compiler in game",
		});
		expect(getCompilerStatus({ ...visible, isRunning: true, languageId: "typescriptreact" })).toStrictEqual({
			text: "$(debug-stop) sloptor",
			tooltip: "Stop the sloptor compiler in game",
		});
	});

	it("should hide off TypeScript, without a folder, or when turned off", ({ expect }) => {
		expect.assertions(4);

		expect(getCompilerStatus({ ...visible, languageId: "lua" })).toBeUndefined();
		expect(getCompilerStatus({ ...visible, languageId: undefined })).toBeUndefined();
		expect(getCompilerStatus({ ...visible, folder: undefined })).toBeUndefined();
		expect(getCompilerStatus({ ...visible, isEnabled: false })).toBeUndefined();
	});
});
