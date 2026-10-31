// Locals: an explicitly annotated `let`, an annotation wider than its
// initializer, an uninitialized `let`, an inferred `const`, a local hoisted
// above its declaration, and bindings imported from a sibling module.
import { aNumber, aString } from "./primitives";

export function locals(): number {
	let count: number = 0;
	let label: string | undefined;
	const names = ["a", "b"];
	for (const name of names) {
		count += name.size();
		label = name;
	}
	return label === undefined ? count : count + label.size();
}

export function widened(flag: boolean): string | number {
	const value: string | number = flag ? "a" : 1;
	return value;
}

export function hoisted(): number {
	return later() + aNumber + aString.size();
}

function later(): number {
	return 1;
}

export function narrowed(write: boolean): "r" | "w" {
	let mode: "r" | "w" = "r";
	if (write) {
		mode = "w";
	}
	return mode;
}
