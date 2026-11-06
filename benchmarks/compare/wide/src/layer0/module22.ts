export enum L0M22Kind {
	Queued,
	Locked,
	Failed,
}

export const L0M22_FACTOR = 17;

export class BaseL0M22 {
	protected total = 48;
	public kind = L0M22Kind.Queued;

	public tally(value: number): number {
		this.total += value * L0M22_FACTOR;
		return this.total;
	}

	public advance(): L0M22Kind {
		const order = [L0M22Kind.Queued, L0M22Kind.Locked, L0M22Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M22(input: number): number {
	let result = input * 17 + 48;
	return result % 42375;
}

export const L0M22_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 48,
	chain: (value) => computeL0M22(value),
};
