export enum L0M24Kind {
	Failed,
	Locked,
	Paused,
	Queued,
	Done,
}

export const L0M24_FACTOR = 17;

export class BaseL0M24 {
	protected total = 69;
	public kind = L0M24Kind.Failed;

	public scale(value: number): number {
		this.total += value * L0M24_FACTOR;
		return this.total;
	}

	public advance(): L0M24Kind {
		const order = [L0M24Kind.Failed, L0M24Kind.Locked, L0M24Kind.Paused, L0M24Kind.Queued, L0M24Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M24(input: number): number {
	let result = input * 17 + 69;
	return result % 66158;
}

export const L0M24_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 69,
	chain: (value) => computeL0M24(value),
};
