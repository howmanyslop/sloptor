import { Color, ColorInformation, ColorPresentation, languages, Range } from "vscode";

import { createColor3ScanCache } from "./color-scan-cache";
import { findColor3MatchAt, formatColor3Presentations } from "./color-scanner";

import type { Disposable, DocumentColorProvider, DocumentSelector } from "vscode";

const DOCUMENT_SELECTOR: DocumentSelector = [{ language: "typescript" }, { language: "typescriptreact" }];

function createColorProvider(getPrecision: () => number): DocumentColorProvider {
	const getMatches = createColor3ScanCache();

	return {
		provideColorPresentations(color, { document, range }): Array<ColorPresentation> {
			const match = findColor3MatchAt(getMatches(document), document.offsetAt(range.start));
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
			const matches = getMatches(document);
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
