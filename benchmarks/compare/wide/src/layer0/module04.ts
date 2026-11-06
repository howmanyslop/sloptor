export enum L0M04Kind {
	Paused,
	Active,
	Idle,
	Done,
}

export const L0M04_FACTOR = 18;

export class BaseL0M04 {
	protected total = 43;
	public kind = L0M04Kind.Paused;

	public shift(value: number): number {
		this.total += value * L0M04_FACTOR;
		return this.total;
	}

	public advance(): L0M04Kind {
		const order = [L0M04Kind.Paused, L0M04Kind.Active, L0M04Kind.Idle, L0M04Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M04(input: number): number {
	let result = input * 18 + 43;
	return result % 61398;
}

export const L0M04_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 43,
	chain: (value) => computeL0M04(value),
};
