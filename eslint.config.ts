import { fileURLToPath } from "node:url";
import { getEslintAsync } from "@rotor-rbx/lint-configurations/get-eslint-async";

const tsconfigPath = fileURLToPath(new URL("tsconfig.json", import.meta.url));
const configuration = await getEslintAsync(tsconfigPath);

export default configuration;
