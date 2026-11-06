export enum L0M25Kind {
	Done,
	Locked,
	Idle,
}

export const L0M25_FACTOR = 4;

export class BaseL0M25 {
	protected total = 84;
	public kind = L0M25Kind.Done;

	public shift(value: number): number {
		this.total += value * L0M25_FACTOR;
		return this.total;
	}

	public advance(): L0M25Kind {
		const order = [L0M25Kind.Done, L0M25Kind.Locked, L0M25Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M25(input: number): number {
	let result = input * 4 + 84;
	return result % 40520;
}

export function tableL0M25(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M25(index));
	return lookup;
}
