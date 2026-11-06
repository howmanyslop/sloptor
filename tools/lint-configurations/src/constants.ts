export const baseIgnores = [
	"**/*.{js,cjs,mjs}",
	".mise/**",
	".vale/**",
	"benchmarks/compare/**",
	"benchmarks/perf-fixture/**",
	"**/testdata/**",
	"documentation/.wrangler/**",
	"include/**",
	"reference/**",
	"test/fixtures/**",
	"rbxts-tsconfig.schema.json",
	"rotor.schema.json",
] satisfies ReadonlyArray<string>;

export const projectType: "app" | "game" | "package" = "package";
