// Enums (plan WP7): a string enum, a numeric enum, a mixed enum, a const enum
// (alias only), an ambient enum (mirrored when referenced), and a string enum
// referenced before its declaration (hoisted).
export enum Direction {
	Up = "up",
	Down = "down",
}

export enum Codes {
	A,
	B,
	C = 10,
}

export enum Mixed {
	One = 1,
	Two = "two",
}

export const enum Flags {
	None = 0,
	Read = 1,
	Write = 2,
}

declare enum Ambient {
	X = 1,
}

export function flip(direction: Direction): Direction {
	return direction === Direction.Up ? Direction.Down : Direction.Up;
}

export function code(): Codes {
	return Codes.B;
}

export function codeName(): string {
	return Codes[Codes.C];
}

export function mixed(): Mixed {
	return Mixed.Two;
}

export function flags(): number {
	return Flags.Read + Flags.Write;
}

export function ambient(value: Ambient): number {
	return value;
}

export function early(): Hoisted {
	return Hoisted.Yes;
}

export enum Hoisted {
	Yes = "yes",
	No = "no",
}
