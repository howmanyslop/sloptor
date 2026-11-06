export enum L0M39Kind {
	Active,
	Idle,
	Queued,
}

export const L0M39_FACTOR = 17;

export class BaseL0M39 {
	protected total = 13;
	public kind = L0M39Kind.Active;

	public shift(value: number): number {
		this.total += value * L0M39_FACTOR;
		return this.total;
	}

	public advance(): L0M39Kind {
		const order = [L0M39Kind.Active, L0M39Kind.Idle, L0M39Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M39(input: number): number {
	let result = input * 17 + 13;
	return result % 14984;
}

export const L0M39_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 13,
	chain: (value) => computeL0M39(value),
};
