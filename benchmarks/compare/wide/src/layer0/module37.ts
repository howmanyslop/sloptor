export enum L0M37Kind {
	Done,
	Locked,
	Idle,
	Queued,
	Paused,
}

export const L0M37_FACTOR = 9;

export class BaseL0M37 {
	protected total = 3;
	public kind = L0M37Kind.Done;

	public fold(value: number): number {
		this.total += value * L0M37_FACTOR;
		return this.total;
	}

	public advance(): L0M37Kind {
		const order = [L0M37Kind.Done, L0M37Kind.Locked, L0M37Kind.Idle, L0M37Kind.Queued, L0M37Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL0M37 extends BaseL0M37 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M37(input: number): number {
	let result = input * 9 + 3;
	return result % 84642;
}

export const L0M37_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 3,
	chain: (value) => computeL0M37(value),
};
