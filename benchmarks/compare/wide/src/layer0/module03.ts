export enum L0M03Kind {
	Paused,
	Done,
	Queued,
}

export const L0M03_FACTOR = 18;

export class BaseL0M03 {
	protected total = 55;
	public kind = L0M03Kind.Paused;

	public scale(value: number): number {
		this.total += value * L0M03_FACTOR;
		return this.total;
	}

	public advance(): L0M03Kind {
		const order = [L0M03Kind.Paused, L0M03Kind.Done, L0M03Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M03(input: number): number {
	let result = input * 18 + 55;
	return result % 25785;
}

export function tableL0M03(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M03(index));
	return lookup;
}
