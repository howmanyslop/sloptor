// Cross-file fixture for imports.ts: one interface, one string-literal alias,
// one generic interface, one class and one function, so type-only, renamed
// and value imports all resolve.
export interface Shape {
	kind: ShapeKind;
	area(): number;
}

export type ShapeKind = "circle" | "square";

export interface Box<T> {
	item: T;
}

export class Circle implements Shape {
	kind: ShapeKind = "circle";

	constructor(public radius: number) {}

	area(): number {
		return math.pi * this.radius * this.radius;
	}
}

export function circle(radius: number): Shape {
	return new Circle(radius);
}

export function describe(shape: Shape): string {
	return `${shape.kind}:${shape.area()}`;
}
