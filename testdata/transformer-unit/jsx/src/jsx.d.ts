// Test-only ambient JSX surface. The "types" array in tsconfig.json excludes
// @rbxts/react's globals so this project controls its own JSX namespace —
// crucially WITHOUT ElementChildrenAttribute, so raw text children typecheck
// and the JsxText fixup pipeline (type-illegal under @rbxts/react, digest
// §1.3) can be exercised through the full transform.
interface ReactElement {
	readonly __element: defined;
}

declare namespace JSX {
	type Element = ReactElement;
	interface IntrinsicElements {
		frame: {
			Visible?: boolean;
			Active?: boolean;
			BackgroundTransparency?: number;
		};
		textlabel: {
			Text?: string;
		};
		"a:b": {
			"c:d"?: string;
		};
	}
}

declare const React: {
	createElement: (...args: Array<unknown>) => ReactElement;
	Fragment: defined;
};
