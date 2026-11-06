export enum L0M20Kind {
	Failed,
	Done,
	Paused,
	Active,
}

export const L0M20_FACTOR = 9;

export class BaseL0M20 {
	protected total = 16;
	public kind = L0M20Kind.Failed;

	public weigh(value: number): number {
		this.total += value * L0M20_FACTOR;
		return this.total;
	}

	public advance(): L0M20Kind {
		const order = [L0M20Kind.Failed, L0M20Kind.Done, L0M20Kind.Paused, L0M20Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M20(input: number): number {
	let result = input * 9 + 16;
	return result % 86644;
}

export function tableL0M20(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M20(index));
	return lookup;
}
