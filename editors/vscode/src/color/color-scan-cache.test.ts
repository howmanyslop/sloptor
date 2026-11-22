import { describe, it } from "vitest";

import { createColor3ScanCache } from "./color-scan-cache";

import type { ScannableDocument } from "./color-scan-cache";

function createDocument(text: string): ScannableDocument {
	return {
		getText(): string {
			return text;
		},
		version: 1,
	};
}

describe("createColor3ScanCache", () => {
	it("should answer the same version with the same scan", ({ expect }) => {
		expect.assertions(2);

		const getMatches = createColor3ScanCache();
		const document = createDocument("Color3.fromRGB(255, 0, 0)");
		const first = getMatches(document);

		expect(first).toHaveLength(1);
		expect(getMatches(document)).toBe(first);
	});

	it("should rescan once the version moves", ({ expect }) => {
		expect.assertions(2);

		const getMatches = createColor3ScanCache();
		let text = "Color3.fromRGB(255, 0, 0)";
		const document = {
			getText(): string {
				return text;
			},
			version: 1,
		};

		expect(getMatches(document)).toHaveLength(1);

		text = "Color3.fromRGB(255, 0, 0); new Color3(0, 1, 0)";
		document.version = 2;

		expect(getMatches(document)).toHaveLength(2);
	});

	it("should keep each document's scan apart", ({ expect }) => {
		expect.assertions(2);

		const getMatches = createColor3ScanCache();

		expect(getMatches(createDocument("new Color3(1, 1, 1)"))).toHaveLength(1);
		expect(getMatches(createDocument("const nothing = 0;"))).toHaveLength(0);
	});
});
