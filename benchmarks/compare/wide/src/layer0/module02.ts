export enum L0M02Kind {
	Paused,
	Done,
	Locked,
	Active,
}

export const L0M02_FACTOR = 14;

export class BaseL0M02 {
	protected total = 58;
	public kind = L0M02Kind.Paused;

	public fold(value: number): number {
		this.total += value * L0M02_FACTOR;
		return this.total;
	}

	public advance(): L0M02Kind {
		const order = [L0M02Kind.Paused, L0M02Kind.Done, L0M02Kind.Locked, L0M02Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M02(input: number): number {
	let result = input * 14 + 58;
	return result % 9967;
}

export function tableL0M02(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M02(index));
	return lookup;
}
