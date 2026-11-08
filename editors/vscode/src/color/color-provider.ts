import { Color, ColorInformation, ColorPresentation, languages, Range } from "vscode";

import { findColor3MatchAt, formatColor3Presentations, scanColor3Calls } from "./color-scanner";

import type { Disposable, DocumentColorProvider, DocumentSelector, TextDocument } from "vscode";

import type { Color3Match } from "./color-scanner";

interface CachedScan {
	readonly matches: ReadonlyArray<Color3Match>;
	readonly version: number;
}

const DOCUMENT_SELECTOR: DocumentSelector = [{ language: "typescript" }, { language: "typescriptreact" }];

function getMatches(cache: WeakMap<TextDocument, CachedScan>, document: TextDocument): ReadonlyArray<Color3Match> {
	const cached = cache.get(document);
	if (cached?.version === document.version) return cached.matches;

	const matches = scanColor3Calls(document.getText());
	cache.set(document, { matches, version: document.version });
	return matches;
}

function createColorProvider(getPrecision: () => number): DocumentColorProvider {
	// VS Code hands back the same TextDocument object while it stays open, so
	// a closed document's scan goes when the document does.
	const cache = new WeakMap<TextDocument, CachedScan>();

	return {
		provideColorPresentations(color, { document, range }): Array<ColorPresentation> {
			const match = findColor3MatchAt(getMatches(cache, document), document.offsetAt(range.start));
			if (match === undefined) {
				return [];
			}

			const labels = formatColor3Presentations([color.red, color.green, color.blue], match, getPrecision());
			const presentations = new Array<ColorPresentation>();
			let size = 0;
			for (const label of labels) {
				presentations[size++] = new ColorPresentation(label);
			}

			return presentations;
		},

		provideDocumentColors(document): Array<ColorInformation> {
			const matches = getMatches(cache, document);
			const colors = new Array<ColorInformation>();
			let size = 0;

			for (const { components, length, offset } of matches) {
				const range = new Range(document.positionAt(offset), document.positionAt(offset + length));
				colors[size++] = new ColorInformation(range, new Color(components[0], components[1], components[2], 1));
			}

			return colors;
		},
	};
}

/**
 * Registers the Color3 color picker for TypeScript and TSX.
 *
 * @param getPrecision - Reads the decimal places to write, on each pick.
 * @returns A Disposable that unregisters the picker.
 */
export function registerColorProvider(getPrecision: () => number): Disposable {
	return languages.registerColorProvider(DOCUMENT_SELECTOR, createColorProvider(getPrecision));
}
