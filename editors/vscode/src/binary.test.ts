import { env, execPath, platform } from "node:process";
import { describe, it } from "vitest";

import { createProcessEnvironment, describeMissingBinary, isExecutableFile, resolveBinary } from "./binary";

import type { BinaryEnvironment, BinaryQuery } from "./binary";

const THIS_FILE = import.meta.filename;

function createPosix(files: ReadonlyArray<string>, path?: string): BinaryEnvironment {
	const existing = new Set(files);
	return {
		environment: path === undefined ? {} : { PATH: path },
		isExecutable(candidate: string): boolean {
			return existing.has(candidate);
		},
		platform: "linux",
	};
}

function createWindows(
	files: ReadonlyArray<string>,
	environment: Readonly<Record<string, string | undefined>>,
): BinaryEnvironment {
	const existing = new Set(files);
	return {
		environment,
		isExecutable(candidate: string): boolean {
			return existing.has(candidate);
		},
		platform: "win32",
	};
}

function createQuery(configuredPath?: string, folder?: string): BinaryQuery {
	return { configuredPath, firstWorkspaceFolder: folder, searchFrom: folder };
}

describe("resolveBinary", () => {
	it("should prefer an absolute configured path", ({ expect }) => {
		expect.assertions(1);

		const environment = createPosix(["/opt/sloptor", "/work/node_modules/.bin/rotor"]);

		expect(resolveBinary(createQuery("/opt/sloptor", "/work"), environment)).toBe("/opt/sloptor");
	});

	it("should resolve a relative configured path against the first workspace folder", ({ expect }) => {
		expect.assertions(1);

		const environment = createPosix(["/work/tools/sloptor"]);
		const query: BinaryQuery = {
			configuredPath: "tools/sloptor",
			firstWorkspaceFolder: "/work",
			searchFrom: "/elsewhere",
		};

		expect(resolveBinary(query, environment)).toBe("/work/tools/sloptor");
	});

	it("should find nothing for a configured path that does not exist", ({ expect }) => {
		expect.assertions(1);

		const environment = createPosix(["/work/node_modules/.bin/rotor", "/usr/bin/sloptor"], "/usr/bin");

		expect(resolveBinary(createQuery("/opt/missing", "/work"), environment)).toBeUndefined();
	});

	it("should find nothing for a relative configured path without a workspace folder", ({ expect }) => {
		expect.assertions(1);

		const environment = createPosix(["/usr/bin/sloptor"], "/usr/bin");

		expect(resolveBinary(createQuery("tools/sloptor", undefined), environment)).toBeUndefined();
	});

	it("should find the nearest node_modules/.bin/rotor walking up", ({ expect }) => {
		expect.assertions(2);

		const environment = createPosix(
			["/repo/node_modules/.bin/rotor", "/repo/packages/game/node_modules/.bin/rotor"],
			"/usr/bin",
		);

		expect(resolveBinary(createQuery(undefined, "/repo/packages/game"), environment)).toBe(
			"/repo/packages/game/node_modules/.bin/rotor",
		);
		expect(resolveBinary(createQuery(undefined, "/repo/packages/other"), environment)).toBe(
			"/repo/node_modules/.bin/rotor",
		);
	});

	it("should prefer sloptor over rotor on PATH, in any directory", ({ expect }) => {
		expect.assertions(1);

		const environment = createPosix(["/a/rotor", "/b/sloptor"], "/a::/b");

		expect(resolveBinary(createQuery(undefined, "/work"), environment)).toBe("/b/sloptor");
	});

	it("should fall back to rotor on PATH", ({ expect }) => {
		expect.assertions(1);

		const environment = createPosix(["/b/rotor"], "/a:/b");

		expect(resolveBinary(createQuery(), environment)).toBe("/b/rotor");
	});

	it("should find nothing without a PATH", ({ expect }) => {
		expect.assertions(1);

		expect(resolveBinary(createQuery(undefined, "/work"), createPosix([]))).toBeUndefined();
	});

	it("should find a Windows shim by extension in node_modules/.bin", ({ expect }) => {
		expect.assertions(2);

		const files = [String.raw`C:\work\node_modules\.bin\rotor.cmd`, String.raw`C:\work\node_modules\.bin\rotor`];

		expect(resolveBinary(createQuery(undefined, String.raw`C:\work`), createWindows(files, {}))).toBe(
			String.raw`C:\work\node_modules\.bin\rotor.cmd`,
		);
		expect(
			resolveBinary(
				createQuery(undefined, String.raw`C:\work`),
				createWindows([...files, String.raw`C:\work\node_modules\.bin\rotor.exe`], {}),
			),
		).toBe(String.raw`C:\work\node_modules\.bin\rotor.exe`);
	});

	it("should search PATH with PATHEXT on Windows, ignoring the case of names", ({ expect }) => {
		expect.assertions(1);

		const environment = createWindows([String.raw`C:\Program Files\sloptor\sloptor.EXE`], {
			Path: String.raw`C:\Windows;"C:\Program Files\sloptor"`,
			PathExt: ".COM;.EXE",
		});

		expect(resolveBinary(createQuery(), environment)).toBe(String.raw`C:\Program Files\sloptor\sloptor.EXE`);
	});

	it("should use the default PATHEXT when Windows has none", ({ expect }) => {
		expect.assertions(1);

		const environment = createWindows([String.raw`C:\bin\rotor.CMD`], { PATH: String.raw`C:\bin` });

		expect(resolveBinary(createQuery(), environment)).toBe(String.raw`C:\bin\rotor.CMD`);
	});

	it("should try PATHEXT on a configured Windows path without an extension", ({ expect }) => {
		expect.assertions(2);

		const environment = createWindows([String.raw`C:\work\bin\sloptor.EXE`, String.raw`D:\sloptor`], {
			PATHEXT: ".EXE",
		});

		expect(resolveBinary(createQuery(String.raw`bin\sloptor`, String.raw`C:\work`), environment)).toBe(
			String.raw`C:\work\bin\sloptor.EXE`,
		);
		expect(resolveBinary(createQuery(String.raw`D:\sloptor`, String.raw`C:\work`), environment)).toBe(
			String.raw`D:\sloptor`,
		);
	});
});

describe("isExecutableFile", () => {
	it("should accept an executable file", ({ expect }) => {
		expect.assertions(1);

		expect(isExecutableFile(execPath, platform)).toBe(true);
	});

	it("should reject directories and missing files", ({ expect }) => {
		expect.assertions(2);

		expect(isExecutableFile(import.meta.dirname, "linux")).toBe(false);
		expect(isExecutableFile(`${THIS_FILE}.missing`, "linux")).toBe(false);
	});

	it("should need the execute bit outside Windows only", ({ expect }) => {
		expect.assertions(2);

		expect(isExecutableFile(THIS_FILE, "linux")).toBe(false);
		expect(isExecutableFile(THIS_FILE, "win32")).toBe(true);
	});
});

describe("createProcessEnvironment", () => {
	it("should describe this process", ({ expect }) => {
		expect.assertions(3);

		const environment = createProcessEnvironment();

		expect(environment.platform).toBe(platform);
		expect(environment.environment).toBe(env);
		expect(environment.isExecutable(execPath)).toBe(true);
	});
});

describe("describeMissingBinary", () => {
	it("should name the lookup when sloptor.path is unset, and the path when it is set", ({ expect }) => {
		expect.assertions(2);

		expect(describeMissingBinary()).toBe(
			"sloptor isn't installed here: there is no node_modules/.bin/rotor and no sloptor or rotor on PATH.",
		);
		expect(describeMissingBinary("./bin/sloptor")).toBe(
			'sloptor.path is "./bin/sloptor", which is not an executable file.',
		);
	});
});
