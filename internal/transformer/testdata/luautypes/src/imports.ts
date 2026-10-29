// Placeholder for WP5: a type-only import and a value import from the same
// sibling module.
import type { Shape, ShapeKind } from "./lib/shapes";
import { Circle, describe } from "./lib/shapes";

export function unit(): Shape {
	return new Circle(1);
}

export function kindOf(shape: Shape): ShapeKind {
	return shape.kind === "circle" ? "circle" : "square";
}

export function label(): string {
	return describe(unit());
}
