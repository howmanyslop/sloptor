export enum L0M33Kind {
	Idle,
	Active,
	Paused,
	Done,
	Failed,
}

export const L0M33_FACTOR = 12;

export class BaseL0M33 {
	protected total = 96;
	public kind = L0M33Kind.Idle;

	public scale(value: number): number {
		this.total += value * L0M33_FACTOR;
		return this.total;
	}

	public advance(): L0M33Kind {
		const order = [L0M33Kind.Idle, L0M33Kind.Active, L0M33Kind.Paused, L0M33Kind.Done, L0M33Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL0M33 extends BaseL0M33 {
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

export function computeL0M33(input: number): number {
	let result = input * 12 + 96;
	return result % 22942;
}

export const L0M33_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 96,
	chain: (value) => computeL0M33(value),
};
