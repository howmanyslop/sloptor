export enum L0M06Kind {
	Failed,
	Queued,
	Paused,
}

export const L0M06_FACTOR = 17;

export class BaseL0M06 {
	protected total = 9;
	public kind = L0M06Kind.Failed;

	public weigh(value: number): number {
		this.total += value * L0M06_FACTOR;
		return this.total;
	}

	public advance(): L0M06Kind {
		const order = [L0M06Kind.Failed, L0M06Kind.Queued, L0M06Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL0M06 extends BaseL0M06 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M06(input: number): number {
	let result = input * 17 + 9;
	return result % 37061;
}

export const L0M06_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 9,
	chain: (value) => computeL0M06(value),
};
