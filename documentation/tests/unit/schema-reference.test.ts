import { describe, it } from "vitest";

import { toSchemaTables } from "$utilities/schema-reference";

import type { JsonSchema } from "$utilities/schema-reference";

const compilerSchema: JsonSchema = {
	properties: {
		flamework: {
			properties: {
				profiles: {
					additionalProperties: {
						properties: {
							optimizations: {
								properties: { guardGenerationDedupLimit: { type: "integer" } },
								type: "object",
							},
							skipUnchangedFiles: { type: "boolean" },
						},
						type: "object",
					},
					type: "object",
				},
			},
			type: "object",
		},
	},
};

describe("schemaTables", () => {
	// Catches a user-named Flamework profile vanishing from the
	// configuration reference, so its keys are documented nowhere.
	it("documents keys of tables the user names, under a placeholder", ({ expect }) => {
		expect.assertions(2);

		const paths = toSchemaTables(compilerSchema).map((table) => table.path);

		expect(paths).toContain("flamework.profiles.<profile>");
		expect(paths).toContain("flamework.profiles.<profile>.optimizations");
	});

	// Catches a nested table's keys being listed under its parent, which would
	// tell users to write them in the wrong TOML section.
	it("lists each key under the table it belongs to", ({ expect }) => {
		expect.assertions(2);

		const tables = toSchemaTables(compilerSchema);
		const optimization = tables.find((table) => table.path === "flamework.profiles.<profile>.optimizations");
		const profile = tables.find((table) => table.path === "flamework.profiles.<profile>");

		expect(optimization?.keys.map((key) => key.name)).toStrictEqual(["guardGenerationDedupLimit"]);
		expect(profile?.keys.map((key) => key.name)).toStrictEqual(["skipUnchangedFiles"]);
	});

	// Catches an enum option being shown as a bare "string", hiding the
	// values a user is allowed to write.
	it("shows the allowed values of an enum", ({ expect }) => {
		expect.assertions(1);

		const [table] = toSchemaTables({
			properties: {
				flamework: {
					properties: { idGenerationMode: { enum: ["full", "short", "obfuscated"], type: "string" } },
					type: "object",
				},
			},
		});

		expect(table?.keys[0]?.type).toBe('"full" | "short" | "obfuscated"');
	});
});
