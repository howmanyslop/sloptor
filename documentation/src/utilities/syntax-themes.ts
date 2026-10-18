import type { ThemeRegistration } from "shiki";

interface SyntaxTones {
	readonly comment: string;
	readonly flag: string;
	readonly ink: string;
	readonly keyword: string;
	readonly punctuation: string;
	readonly value: string;
	readonly well: string;
}

/**
 * Builds a Shiki theme from the site's syntax tones. In a shell line the command stays ink, the subcommand is teal,
 * flags are amber, and values are raspberry. Every tone is at least 4.5:1 against the well and the paper, and none of
 * them reuses the grape (sloptor) or TypeScript blue (rbxtsc) data colors. Keep in sync with the `--syntax-*` tokens in
 * `custom.css`.
 *
 * @param name - The Shiki theme name.
 * @param type - Whether the theme is for the light or dark scheme.
 * @param tones - The colors for each token kind.
 * @returns A Shiki theme registration.
 */
function createSyntaxTheme(name: string, type: "dark" | "light", tones: SyntaxTones): ThemeRegistration {
	return {
		colors: { "editor.background": tones.well, "editor.foreground": tones.ink },
		name,
		settings: [
			{ settings: { foreground: tones.ink } },
			{
				scope: ["comment", "punctuation.definition.comment"],
				settings: { fontStyle: "italic", foreground: tones.comment },
			},
			{
				scope: ["entity.name.command", "entity.name.function.call.shell", "support.function.builtin.shell"],
				settings: { foreground: tones.ink },
			},
			{
				scope: [
					"string.unquoted.argument.shell",
					"keyword",
					"storage",
					"storage.type",
					"storage.modifier",
					"keyword.control",
					"entity.name.type",
					"entity.name.class",
					"support.type",
					"support.class",
					"entity.name.tag",
					"entity.name.function",
					"support.function",
				],
				settings: { foreground: tones.keyword },
			},
			{
				scope: [
					"constant.other.option",
					"support.type.property-name",
					"meta.object-literal.key",
					"entity.other.attribute-name",
					"entity.name.label",
					"variable.other.readwrite.alias",
				],
				settings: { foreground: tones.flag },
			},
			{
				scope: [
					"string",
					"string.quoted",
					"string.template",
					"punctuation.definition.string",
					"constant.numeric",
					"constant.language",
					"constant.character",
					"support.constant",
					"variable.other.constant",
					"string.unquoted.argument.shell constant.numeric",
				],
				settings: { foreground: tones.value },
			},
			{
				scope: ["punctuation", "meta.brace", "keyword.operator"],
				settings: { foreground: tones.punctuation },
			},
		],
		type,
	};
}

export const syntaxLightThemeName = "sloptor-light";
export const syntaxDarkThemeName = "sloptor-dark";

export const syntaxLightTheme = createSyntaxTheme(syntaxLightThemeName, "light", {
	comment: "#6b7079",
	flag: "#8a5200",
	ink: "#1a1c20",
	keyword: "#0f6b6b",
	punctuation: "#555a63",
	value: "#a12d4c",
	well: "#ffffff",
});

export const syntaxDarkTheme = createSyntaxTheme(syntaxDarkThemeName, "dark", {
	comment: "#8c9199",
	flag: "#e6b35c",
	ink: "#eeeeec",
	keyword: "#6fd0c8",
	punctuation: "#a9adb4",
	value: "#f08aa4",
	well: "#171a1d",
});
