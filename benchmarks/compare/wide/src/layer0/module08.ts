export enum L0M08Kind {
	Done,
	Idle,
	Active,
	Failed,
	Paused,
}

export const L0M08_FACTOR = 14;

export class BaseL0M08 {
	protected total = 22;
	public kind = L0M08Kind.Done;

	public fold(value: number): number {
		this.total += value * L0M08_FACTOR;
		return this.total;
	}

	public advance(): L0M08Kind {
		const order = [L0M08Kind.Done, L0M08Kind.Idle, L0M08Kind.Active, L0M08Kind.Failed, L0M08Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M08(input: number): number {
	let result = input * 14 + 22;
	return result % 22949;
}

export function tableL0M08(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M08(index));
	return lookup;
}
