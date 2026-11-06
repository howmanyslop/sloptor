export enum L0M19Kind {
	Locked,
	Active,
	Idle,
	Queued,
	Failed,
}

export const L0M19_FACTOR = 10;

export class BaseL0M19 {
	protected total = 81;
	public kind = L0M19Kind.Locked;

	public mix(value: number): number {
		this.total += value * L0M19_FACTOR;
		return this.total;
	}

	public advance(): L0M19Kind {
		const order = [L0M19Kind.Locked, L0M19Kind.Active, L0M19Kind.Idle, L0M19Kind.Queued, L0M19Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M19(input: number): number {
	let result = input * 10 + 81;
	return result % 9350;
}

export const L0M19_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 81,
	chain: (value) => computeL0M19(value),
};
