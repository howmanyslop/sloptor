// Cross-file types: a type-only import, a renamed import of a generic
// interface, value imports from the same sibling module, and a lib type
// (`Promise<number>`). Every imported type is mirrored as a hoisted,
// non-exported alias under its local name, because `TS.import` is opaque to
// Luau's checker.
import type { Shape, ShapeKind, Box as Container } from "./lib/shapes";
import { circle, describe } from "./lib/shapes";

export interface Wrapped {
	shape: Shape;
	kind: ShapeKind;
	container: Container<Shape>;
	pending: Promise<number>;
}

export function unit(): Shape {
	return circle(1);
}

export function kindOf(shape: Shape): ShapeKind {
	return shape.kind === "circle" ? "circle" : "square";
}

export function wrap(shape: Shape): Container<Shape> {
	return { item: shape };
}

export function label(): string {
	return describe(unit());
}
