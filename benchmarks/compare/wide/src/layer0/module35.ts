export enum L0M35Kind {
	Active,
	Idle,
	Done,
}

export const L0M35_FACTOR = 6;

export class BaseL0M35 {
	protected total = 81;
	public kind = L0M35Kind.Active;

	public blend(value: number): number {
		this.total += value * L0M35_FACTOR;
		return this.total;
	}

	public advance(): L0M35Kind {
		const order = [L0M35Kind.Active, L0M35Kind.Idle, L0M35Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M35(input: number): number {
	let result = input * 6 + 81;
	return result % 10905;
}

export function tableL0M35(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M35(index));
	return lookup;
}
