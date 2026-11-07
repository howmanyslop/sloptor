export enum L0M13Kind {
	Idle,
	Failed,
	Queued,
	Done,
}

export const L0M13_FACTOR = 19;

export class BaseL0M13 {
	protected total = 68;
	public kind = L0M13Kind.Idle;

	public mix(value: number): number {
		this.total += value * L0M13_FACTOR;
		return this.total;
	}

	public advance(): L0M13Kind {
		const order = [L0M13Kind.Idle, L0M13Kind.Failed, L0M13Kind.Queued, L0M13Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL0M13 extends BaseL0M13 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M13(input: number): number {
	let result = input * 19 + 68;
	return result % 53092;
}

export function tableL0M13(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M13(index));
	return lookup;
}
