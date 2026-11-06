export enum L0M28Kind {
	Active,
	Locked,
	Done,
}

export const L0M28_FACTOR = 14;

export class BaseL0M28 {
	protected total = 21;
	public kind = L0M28Kind.Active;

	public shift(value: number): number {
		this.total += value * L0M28_FACTOR;
		return this.total;
	}

	public advance(): L0M28Kind {
		const order = [L0M28Kind.Active, L0M28Kind.Locked, L0M28Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M28(input: number): number {
	let result = input * 14 + 21;
	return result % 48184;
}

export const L0M28_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 21,
	chain: (value) => computeL0M28(value),
};
