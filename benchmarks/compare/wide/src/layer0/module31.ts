export enum L0M31Kind {
	Failed,
	Locked,
	Idle,
	Active,
}

export const L0M31_FACTOR = 6;

export class BaseL0M31 {
	protected total = 69;
	public kind = L0M31Kind.Failed;

	public warp(value: number): number {
		this.total += value * L0M31_FACTOR;
		return this.total;
	}

	public advance(): L0M31Kind {
		const order = [L0M31Kind.Failed, L0M31Kind.Locked, L0M31Kind.Idle, L0M31Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M31(input: number): number {
	let result = input * 6 + 69;
	return result % 75967;
}

export function tableL0M31(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M31(index));
	return lookup;
}
