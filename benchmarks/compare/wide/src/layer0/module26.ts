export enum L0M26Kind {
	Idle,
	Active,
	Paused,
	Done,
}

export const L0M26_FACTOR = 2;

export class BaseL0M26 {
	protected total = 43;
	public kind = L0M26Kind.Idle;

	public shift(value: number): number {
		this.total += value * L0M26_FACTOR;
		return this.total;
	}

	public advance(): L0M26Kind {
		const order = [L0M26Kind.Idle, L0M26Kind.Active, L0M26Kind.Paused, L0M26Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M26(input: number): number {
	let result = input * 2 + 43;
	return result % 78402;
}

export function tableL0M26(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M26(index));
	return lookup;
}
