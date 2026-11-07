import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, it } from "vitest";

import { computeClosure, mirror, readPackages, trimWorkspaceManifest } from "./mirror.ts";

const WORKSPACE = `[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "1.2.3"
edition = "2024"

[workspace.metadata.typescript]
version = "7.1.0-beta"
commit = "0123456789abcdef"

[workspace.dependencies]
tsrs_a = { path = "crates/tsrs_a" }
tsrs_b = { path = "crates/tsrs_b" }
tsrs_c = { path = "crates/tsrs_c" }

[profile.release]
lto = "fat"

# Shared lint policy.
[workspace.lints.rust]
unsafe_code = "deny"
`;

function createManifest(
	name: string,
	dependencies: ReadonlyArray<string>,
	devDependencies: ReadonlyArray<string> = [],
): string {
	function formatDependencies(names: ReadonlyArray<string>): string {
		return names.map((dependency) => `${dependency} = { workspace = true }`).join("\n");
	}

	return `[package]
name = "${name}"
version.workspace = true
edition.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
${formatDependencies(dependencies)}

[dev-dependencies]
${formatDependencies(devDependencies)}

[lints]
workspace = true
`;
}

function write(file: string, content: string): void {
	mkdirSync(path.dirname(file), { recursive: true });
	writeFileSync(file, content);
}

/**
 * Builds a fake checkout with two chained patches, runs `run`, cleans up.
 *
 * @template Result - What `run` returns.
 * @param run - Receives the temp root and the fake checkout.
 * @returns Whatever `run` returns.
 */
function inFixture<Result>(run: (root: string, source: string) => Result): Result {
	const root = mkdtempSync(path.join(tmpdir(), "tsrs-mirror-test-"));
	try {
		const source = path.join(root, "source");
		write(path.join(source, "Cargo.toml"), WORKSPACE);
		write(path.join(source, "LICENSE"), "license text\n");
		write(path.join(source, "NOTICE"), "notice text\n");
		// a is a dev-dependency of b, so b's closure must keep it. c is
		// unreachable.
		write(path.join(source, "crates/tsrs_a/Cargo.toml"), createManifest("tsrs_a", []));
		write(path.join(source, "crates/tsrs_a/src/lib.rs"), "pub fn value() -> u32 {\n    1\n}\n");
		write(path.join(source, "crates/tsrs_b/Cargo.toml"), createManifest("tsrs_b", [], ["tsrs_a"]));
		write(path.join(source, "crates/tsrs_b/src/lib.rs"), "");
		write(path.join(source, "crates/tsrs_c/Cargo.toml"), createManifest("tsrs_c", ["tsrs_a"]));
		write(path.join(source, "crates/tsrs_c/src/lib.rs"), "");
		write(path.join(source, "crates/tsrs_b/target/junk.txt"), "build output");

		// 0002 only applies on top of 0001, so applying out of order would fail.
		write(
			path.join(root, "patches/0001-first.patch"),
			"--- a/crates/tsrs_a/src/lib.rs\n+++ b/crates/tsrs_a/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn value() -> u32 {\n-    1\n+    2\n }\n",
		);
		write(
			path.join(root, "patches/0002-second.patch"),
			"--- a/crates/tsrs_a/src/lib.rs\n+++ b/crates/tsrs_a/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn value() -> u32 {\n-    2\n+    3\n }\n",
		);
		return run(root, source);
	} finally {
		rmSync(root, { force: true, recursive: true });
	}
}

describe("computeClosure", () => {
	it("follows path dependencies and ignores registry ones", ({ expect }) => {
		expect.assertions(1);

		const packages = [
			{ name: "x", dependencies: [{ name: "y", path: "/w/y" }, { name: "serde" }] },
			{ name: "y", dependencies: [{ name: "z", path: "/w/z" }] },
			{ name: "z", dependencies: [] },
			{ name: "unused", dependencies: [] },
		];

		expect(computeClosure(packages, ["x"])).toStrictEqual(["x", "y", "z"]);
	});

	it("rejects roots missing from the workspace", ({ expect }) => {
		expect.assertions(1);
		expect(() => computeClosure([], ["missing"])).toThrow(/missing/u);
	});
});

describe("trimWorkspaceManifest", () => {
	it("keeps inherited sections and drops unused crate paths and profiles", ({ expect }) => {
		expect.assertions(6);

		const trimmed = trimWorkspaceManifest(WORKSPACE, ["tsrs_a"]);

		expect(trimmed).toMatch(/\[workspace\.lints\.rust\]/u);
		expect(trimmed).toMatch(/# Shared lint policy\./u);
		expect(trimmed).toMatch(/tsrs_a = \{ path/u);
		expect(trimmed).not.toMatch(/tsrs_b/u);
		expect(trimmed).not.toMatch(/profile/u);
		expect(trimmed).toMatch(/members = \["crates\/tsrs_a"\]/u);
	});
});

describe("mirror", () => {
	it("copies the closure, applies patches in order and records the commit", ({ expect }) => {
		expect.assertions(10);

		const result = inFixture((root, source) => {
			const out = path.join(root, "out");
			const mirrored = mirror({
				commit: "deadbeef",
				out,
				patchesDirectory: path.join(root, "patches"),
				roots: ["tsrs_b"],
				source,
				tag: "v9.9.9",
			});

			return {
				crates: mirrored.crates,
				hasCrateC: existsSync(path.join(out, "crates/tsrs_c")),
				hasTarget: existsSync(path.join(out, "crates/tsrs_b/target")),
				library: readFileSync(path.join(out, "crates/tsrs_a/src/lib.rs"), "utf8"),
				license: readFileSync(path.join(out, "LICENSE"), "utf8"),
				notes: readFileSync(path.join(out, "MIRROR.md"), "utf8"),
				notice: readFileSync(path.join(out, "NOTICE"), "utf8"),
				// Cargo only loads the trimmed root if `workspace = true`
				// resolves.
				packages: readPackages(path.join(out, "Cargo.toml")).map((package_) => package_.name),
			};
		});

		expect(result.crates).toStrictEqual(["tsrs_a", "tsrs_b"]);
		expect(result.hasCrateC).toBe(false);
		expect(result.hasTarget).toBe(false);
		expect(result.license).toBe("license text\n");
		expect(result.notice).toBe("notice text\n");
		expect(result.library).toMatch(/^ {4}3$/mu);
		expect(result.notes).toMatch(/deadbeef/u);
		expect(result.notes).toMatch(/v9\.9\.9/u);
		expect(result.notes).toMatch(/0002-second\.patch/u);
		expect(result.packages).toStrictEqual(["tsrs_a", "tsrs_b"]);
	});

	it("replaces a previous mirror instead of merging into it", ({ expect }) => {
		expect.assertions(1);

		const hasStale = inFixture((root, source) => {
			const out = path.join(root, "out-stale");
			write(path.join(out, "stale.txt"), "left over");
			mirror({
				commit: "x",
				out,
				patchesDirectory: path.join(root, "patches"),
				roots: ["tsrs_a"],
				source,
				tag: "y",
			});
			return existsSync(path.join(out, "stale.txt"));
		});

		expect(hasStale).toBe(false);
	});
});
