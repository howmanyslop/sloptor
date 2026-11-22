import { describe, it } from "vitest";

import { describeExit, toTerminalText } from "./terminal-text";

describe("toTerminalText", () => {
	it("should end every line with a carriage return and line feed", ({ expect }) => {
		expect.assertions(1);

		expect(toTerminalText("a\nb\r\nc\n")).toBe("a\r\nb\r\nc\r\n");
	});

	it("should leave text without line feeds alone", ({ expect }) => {
		expect.assertions(1);

		expect(toTerminalText("watching\r")).toBe("watching\r");
	});
});

describe("describeExit", () => {
	it("should treat exit code zero as success", ({ expect }) => {
		expect.assertions(1);

		expect(describeExit(0, null)).toStrictEqual({ failed: false, message: "sloptor exited with code 0." });
	});

	it("should treat a non-zero exit code as failure", ({ expect }) => {
		expect.assertions(1);

		expect(describeExit(2, null)).toStrictEqual({ failed: true, message: "sloptor exited with code 2." });
	});

	it("should treat a signal as failure", ({ expect }) => {
		expect.assertions(1);

		expect(describeExit(null, "SIGKILL")).toStrictEqual({
			failed: true,
			message: "sloptor was killed by SIGKILL.",
		});
	});

	it("should treat an exit with neither as failure", ({ expect }) => {
		expect.assertions(1);

		expect(describeExit(null, null)).toStrictEqual({ failed: true, message: "sloptor exited." });
	});
});
