import { scanColor3Calls } from "./color-scanner";

import type { Color3Match } from "./color-scanner";

/** The part of a VS Code `TextDocument` a scan reads. */
export interface ScannableDocument {
	getText(): string;
	readonly version: number;
}

interface CachedScan {
	readonly matches: ReadonlyArray<Color3Match>;
	readonly version: number;
}

export type GetColor3Matches = (document: ScannableDocument) => ReadonlyArray<Color3Match>;

/**
 * VS Code asks for the colors on every edit and scroll, and again for the
 * presentations of each pick; one scan per document version answers all of
 * them. VS Code hands back the same document object while it stays open, so a
 * closed document's scan goes when the document does.
 *
 * @returns Reads a document's matches, scanning only when its version moved.
 */
export function createColor3ScanCache(): GetColor3Matches {
	const cache = new WeakMap<ScannableDocument, CachedScan>();

	return function getColor3Matches(document) {
		const cached = cache.get(document);
		if (cached?.version === document.version) return cached.matches;

		const matches = scanColor3Calls(document.getText());
		cache.set(document, { matches, version: document.version });
		return matches;
	};
}
