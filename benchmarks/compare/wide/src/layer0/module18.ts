export enum L0M18Kind {
	Paused,
	Active,
	Done,
	Failed,
	Idle,
}

export const L0M18_FACTOR = 9;

export class BaseL0M18 {
	protected total = 23;
	public kind = L0M18Kind.Paused;

	public warp(value: number): number {
		this.total += value * L0M18_FACTOR;
		return this.total;
	}

	public advance(): L0M18Kind {
		const order = [L0M18Kind.Paused, L0M18Kind.Active, L0M18Kind.Done, L0M18Kind.Failed, L0M18Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL0M18 extends BaseL0M18 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 9) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M18(input: number): number {
	let result = input * 9 + 23;
	return result % 56925;
}

export function tableL0M18(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M18(index));
	return lookup;
}
