import { fileURLToPath } from "node:url";
import { getOxlintAsync } from "@rotor-rbx/lint-configurations/get-oxlint-async";

import type { OxlintConfig } from "oxlint";

const tsconfigPath = fileURLToPath(new URL("tsconfig.json", import.meta.url));

// eslint-disable-next-line flawless/no-redundant-type-annotation -- the inferred type is not portable (TS2883).
const configuration: OxlintConfig = await getOxlintAsync({
	rootDirectory: import.meta.dirname,
	tsconfigPath,
});

export default configuration;
