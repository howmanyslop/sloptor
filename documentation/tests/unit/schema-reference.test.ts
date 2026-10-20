import { describe, it } from "vitest";

import { toSchemaTables } from "$utilities/schema-reference";

import type { JsonSchema } from "$utilities/schema-reference";

const deploySchema: JsonSchema = {
	properties: {
		deploy: {
			description: "Deployment.",
			properties: {
				environments: {
					additionalProperties: {
						properties: {
							places: {
								additionalProperties: {
									properties: { placeId: { description: "The place id.", type: "integer" } },
									type: "object",
								},
								type: "object",
							},
							universeId: { description: "The universe id.", type: "integer" },
						},
						type: "object",
					},
					description: "Environments by name.",
					type: "object",
				},
			},
			type: "object",
		},
	},
	type: "object",
};

describe("schemaTables", () => {
	// Catches a user-named table (an environment, a place) vanishing from the
	// configuration reference, so its keys are documented nowhere.
	it("documents keys of tables the user names, under a placeholder", ({ expect }) => {
		expect.assertions(2);

		const paths = toSchemaTables(deploySchema).map((table) => table.path);

		expect(paths).toContain("deploy.environments.<environment>");
		expect(paths).toContain("deploy.environments.<environment>.places.<place>");
	});

	// Catches a nested table's keys being listed under its parent, which would
	// tell users to write them in the wrong TOML section.
	it("lists each key under the table it belongs to", ({ expect }) => {
		expect.assertions(2);

		const tables = toSchemaTables(deploySchema);
		const place = tables.find((table) => table.path === "deploy.environments.<environment>.places.<place>");
		const environment = tables.find((table) => table.path === "deploy.environments.<environment>");

		expect(place?.keys.map((key) => key.name)).toStrictEqual(["placeId"]);
		expect(environment?.keys.map((key) => key.name)).toStrictEqual(["universeId"]);
	});

	// Catches an enum option being shown as a bare "string", hiding the
	// values a user is allowed to write.
	it("shows the allowed values of an enum", ({ expect }) => {
		expect.assertions(1);

		const [table] = toSchemaTables({
			properties: {
				assets: { properties: { mode: { enum: ["module", "macro"], type: "string" } }, type: "object" },
			},
		});

		expect(table?.keys[0]?.type).toBe('"module" | "macro"');
	});
});
