import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, it } from "vitest";

import { applyAllowlist, compileGlob, diffTrees, parseAllowlist } from "./diff.ts";

function inTrees<Result>(
	run: (put: (tree: string, relative: string, content: string) => void, root: string) => Result,
): Result {
	const root = mkdtempSync(path.join(tmpdir(), "parity-test-"));
	try {
		return run((tree, relative, content) => {
			const absolute = path.join(root, tree, relative);
			mkdirSync(path.dirname(absolute), { recursive: true });
			writeFileSync(absolute, content);
		}, root);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
}

describe("diffTrees", () => {
	it("reports changed and one-sided files, and nothing for identical ones", ({ expect }) => {
		expect.assertions(1);

		const differences = inTrees((put, root) => {
			put("a", "out/same.luau", "x");
			put("b", "out/same.luau", "x");
			put("a", "out/changed.luau", "old");
			put("b", "out/changed.luau", "new");
			put("a", "out/gone.luau", "x");
			put("b", "out/new.luau", "x");
			return diffTrees(path.join(root, "a"), path.join(root, "b"));
		});

		expect(differences).toStrictEqual([
			{ kind: "changed", path: "out/changed.luau" },
			{ kind: "only-baseline", path: "out/gone.luau" },
			{ kind: "only-candidate", path: "out/new.luau" },
		]);
	});

	it("skips the cache, dependency and copyfiles bookkeeping", ({ expect }) => {
		expect.assertions(1);

		const differences = inTrees((put, root) => {
			put("a", ".rotor/state.json", "1");
			put("b", ".rotor/state.json", "2");
			put("a", "node_modules/pkg/index.js", "1");
			put("a", "rbxts.copyfiles.json", "1");
			put("b", "rbxts.copyfiles.json", "2");
			symlinkSync("/nowhere", path.join(root, "b/node_modules-link"));
			return diffTrees(path.join(root, "a"), path.join(root, "b"));
		});

		expect(differences).toStrictEqual([{ kind: "only-candidate", path: "node_modules-link" }]);
	});

	it("treats a file that became a directory as a change", ({ expect }) => {
		expect.assertions(1);

		const paths = inTrees((put, root) => {
			put("a", "out/x", "file");
			put("b", "out/x/inner.luau", "dir");
			return diffTrees(path.join(root, "a"), path.join(root, "b")).map((difference) => difference.path);
		});

		expect(paths).toStrictEqual(["out/x", "out/x/inner.luau"]);
	});
});

describe("allowlist", () => {
	it("ignores comments and blank lines, including trailing reasons", ({ expect }) => {
		expect.assertions(1);

		const entries = parseAllowlist("# header\n\nout/**/*.d.ts # TS 7.1 emit\n  sourcemap.json  \n");

		expect(entries.map((entry) => [entry.pattern, entry.line])).toStrictEqual([
			["out/**/*.d.ts", 3],
			["sourcemap.json", 4],
		]);
	});

	it("matches * within a segment and ** across segments", ({ expect }) => {
		expect.assertions(5);
		expect(compileGlob("out/*.d.ts").test("out/a.d.ts")).toBe(true);
		expect(compileGlob("out/*.d.ts").test("out/deep/a.d.ts")).toBe(false);
		expect(compileGlob("out/**/*.d.ts").test("out/deep/er/a.d.ts")).toBe(true);
		expect(compileGlob("out/**/*.d.ts").test("out/a.d.ts")).toBe(true);
		expect(compileGlob("a.b").test("axb")).toBe(false);
	});

	it("separates unlisted differences from allowed ones and flags stale entries", ({ expect }) => {
		expect.assertions(3);

		const report = applyAllowlist(
			[
				{ kind: "changed", path: "out/a.d.ts" },
				{ kind: "changed", path: "out/a.luau" },
			],
			parseAllowlist("out/*.d.ts\nout/never.map\n"),
		);

		expect(report.unlisted).toStrictEqual([{ kind: "changed", path: "out/a.luau" }]);
		expect(report.allowed).toStrictEqual([{ kind: "changed", path: "out/a.d.ts" }]);
		expect(report.unusedEntries.map((entry) => entry.pattern)).toStrictEqual(["out/never.map"]);
	});

	it("passes when there are no differences at all", ({ expect }) => {
		expect.assertions(1);

		const report = applyAllowlist([], parseAllowlist(""));

		expect(report.unlisted).toHaveLength(0);
	});
});
