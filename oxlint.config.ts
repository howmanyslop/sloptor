import { fileURLToPath } from "node:url";
import { getOxlintAsync } from "@rotor-rbx/lint-configurations/get-oxlint-async";

const tsconfigPath = fileURLToPath(new URL("tsconfig.json", import.meta.url));
const configuration = await getOxlintAsync({ tsconfigPath });

export default configuration;
