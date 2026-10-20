import { readFile } from "node:fs/promises";

import type { OxfmtOptions } from "@isentinel/eslint-config";

type OxfmtConfiguration = OxfmtOptions & { readonly $schema?: string };

const OXFMT_CONFIGURATION_URL = new URL("../../../.oxfmtrc.json", import.meta.url);

function isOxfmtConfiguration(value: unknown): value is OxfmtConfiguration {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

export async function getOxfmtConfigurationAsync(): Promise<OxfmtOptions> {
	const parsed: unknown = JSON.parse(await readFile(OXFMT_CONFIGURATION_URL, "utf8"));
	if (!isOxfmtConfiguration(parsed)) throw new TypeError(".oxfmtrc.json must contain an object.");

	const { $schema: _, ...configuration } = parsed;
	return configuration;
}
