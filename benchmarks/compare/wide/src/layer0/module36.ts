export enum L0M36Kind {
	Failed,
	Done,
	Paused,
	Active,
	Idle,
}

export const L0M36_FACTOR = 3;

export class BaseL0M36 {
	protected total = 21;
	public kind = L0M36Kind.Failed;

	public scale(value: number): number {
		this.total += value * L0M36_FACTOR;
		return this.total;
	}

	public advance(): L0M36Kind {
		const order = [L0M36Kind.Failed, L0M36Kind.Done, L0M36Kind.Paused, L0M36Kind.Active, L0M36Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL0M36 extends BaseL0M36 {
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

export function computeL0M36(input: number): number {
	let result = input * 3 + 21;
	return result % 66234;
}

export function tableL0M36(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M36(index));
	return lookup;
}
