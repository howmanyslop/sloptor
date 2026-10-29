// Placeholder for WP7: string, numeric and const enums.
export enum Direction {
	Up = "up",
	Down = "down",
}

export enum Codes {
	A,
	B,
	C = 10,
}

export const enum Flags {
	None = 0,
	Read = 1,
	Write = 2,
}

export function flip(direction: Direction): Direction {
	return direction === Direction.Up ? Direction.Down : Direction.Up;
}

export function code(): Codes {
	return Codes.B;
}

export function flags(): number {
	return Flags.Read + Flags.Write;
}
