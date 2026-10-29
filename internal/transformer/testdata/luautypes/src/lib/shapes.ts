// Cross-file fixture for imports.ts: one interface, one string-literal alias,
// one class and one function, so type-only and value imports both resolve.
export interface Shape {
	kind: ShapeKind;
	area(): number;
}

export type ShapeKind = "circle" | "square";

export class Circle implements Shape {
	kind: ShapeKind = "circle";

	constructor(public radius: number) {}

	area(): number {
		return math.pi * this.radius * this.radius;
	}
}

export function describe(shape: Shape): string {
	return `${shape.kind}:${shape.area()}`;
}
