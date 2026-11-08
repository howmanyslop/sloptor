import { describe, it } from "vitest";

import { createSpawnCommand } from "./spawn-command";

describe("createSpawnCommand", () => {
	it("should pass argv through untouched outside Windows", ({ expect }) => {
		expect.assertions(1);

		expect(createSpawnCommand("/usr/bin/sloptor.cmd", ["build", "--watch"], "darwin")).toStrictEqual({
			arguments: ["build", "--watch"],
			command: "/usr/bin/sloptor.cmd",
			shell: false,
		});
	});

	it("should pass argv through untouched for a Windows executable", ({ expect }) => {
		expect.assertions(1);

		expect(createSpawnCommand(String.raw`C:\bin\sloptor.exe`, ["lsp", "--stdio"], "win32")).toStrictEqual({
			arguments: ["lsp", "--stdio"],
			command: String.raw`C:\bin\sloptor.exe`,
			shell: false,
		});
	});

	it("should copy the arguments", ({ expect }) => {
		expect.assertions(1);

		const commandArguments = ["build"];

		expect(createSpawnCommand("/usr/bin/sloptor", commandArguments, "linux").arguments).not.toBe(commandArguments);
	});

	it("should escape a batch file into one cmd line", ({ expect }) => {
		expect.assertions(1);

		const binary = String.raw`C:\Program Files\sloptor\sloptor.CMD`;
		const commandArguments = ["build", 'say "hi"', String.raw`a\b\\`.slice(0, -1), "%PATH%"];

		expect(createSpawnCommand(binary, commandArguments, "win32")).toStrictEqual({
			arguments: [],
			command: String.raw`C:\Program^ Files\sloptor\sloptor.CMD ^"build^" ^"say^ \^"hi\^"^" ^"a\b\\^" ^"^%PATH^%^"`,
			shell: true,
		});
	});

	it("should escape twice for an npm shim", ({ expect }) => {
		expect.assertions(1);

		const binary = String.raw`C:\work\node_modules\.bin\rotor.cmd`;

		expect(createSpawnCommand(binary, ["build", "a&b"], "win32")).toStrictEqual({
			arguments: [],
			command: String.raw`C:\work\node_modules\.bin\rotor.cmd ^^^"build^^^" ^^^"a^^^&b^^^"`,
			shell: true,
		});
	});
});
