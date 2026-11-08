import { describe, it } from "vitest";

import { describeOutputPathError, describeOutputPathProblem, parseOutputPathAnswer } from "./output-path";

describe("parseOutputPathAnswer", () => {
	it("should read a path", ({ expect }) => {
		expect.assertions(1);

		expect(parseOutputPathAnswer({ path: "/work/out/init.luau" })).toStrictEqual({
			kind: "path",
			path: "/work/out/init.luau",
		});
	});

	it("should read null as not a TypeScript file", ({ expect }) => {
		expect.assertions(1);

		expect(parseOutputPathAnswer(null)).toStrictEqual({ kind: "not-typescript" });
	});

	it("should reject shapes the server never sends", ({ expect }) => {
		expect.assertions(5);

		expect(parseOutputPathAnswer("/work/out/init.luau")).toStrictEqual({ kind: "malformed" });
		expect(parseOutputPathAnswer(["/work/out/init.luau"])).toStrictEqual({ kind: "malformed" });
		expect(parseOutputPathAnswer({})).toStrictEqual({ kind: "malformed" });
		expect(parseOutputPathAnswer({ path: 1 })).toStrictEqual({ kind: "malformed" });
		expect(parseOutputPathAnswer({ path: "" })).toStrictEqual({ kind: "malformed" });
	});
});

describe("describeOutputPathError", () => {
	it("should pass on the server's reason for RequestFailed", ({ expect }) => {
		expect.assertions(1);

		const error = Object.assign(new Error("no tsconfig.json owns /work/a.ts"), { code: -32_803 });

		expect(describeOutputPathError(error)).toBe("sloptor can't find the output: no tsconfig.json owns /work/a.ts");
	});

	it("should report other errors as failures", ({ expect }) => {
		expect.assertions(4);

		const invalid = Object.assign(new Error("invalid params"), { code: -32_602 });

		expect(describeOutputPathError(invalid)).toBe("sloptor/outputPath failed: invalid params");
		expect(describeOutputPathError({ code: -32_803, message: 1 })).toBe("sloptor/outputPath failed.");
		expect(describeOutputPathError({ code: -32_803 })).toBe("sloptor/outputPath failed.");
		expect(describeOutputPathError(null)).toBe("sloptor/outputPath failed.");
	});
});

describe("describeOutputPathProblem", () => {
	it("should say why there is nothing to open", ({ expect }) => {
		expect.assertions(2);

		expect(describeOutputPathProblem({ kind: "not-typescript" })).toBe(
			"Open Output only works on a TypeScript source file.",
		);
		expect(describeOutputPathProblem({ kind: "malformed" })).toBe(
			"sloptor/outputPath answered with something that is not a path.",
		);
	});
});
