export enum L0M15Kind {
	Paused,
	Locked,
	Active,
}

export const L0M15_FACTOR = 4;

export class BaseL0M15 {
	protected total = 74;
	public kind = L0M15Kind.Paused;

	public tally(value: number): number {
		this.total += value * L0M15_FACTOR;
		return this.total;
	}

	public advance(): L0M15Kind {
		const order = [L0M15Kind.Paused, L0M15Kind.Locked, L0M15Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M15(input: number): number {
	let result = input * 4 + 74;
	return result % 95213;
}

export const L0M15_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 74,
	chain: (value) => computeL0M15(value),
};
