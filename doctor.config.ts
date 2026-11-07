import { defineConfig } from "react-doctor/api";

const configuration = defineConfig({
	categories: {},
	ignore: {
		overrides: [],
	},
	rules: {
		"deslop/unused-dependency": "off",
		"deslop/unused-dev-dependency": "off",
		"deslop/unused-export": "off",
		"deslop/unused-file": "off",
	},
});

export default configuration;
