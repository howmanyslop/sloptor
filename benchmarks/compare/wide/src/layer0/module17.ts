export enum L0M17Kind {
	Paused,
	Active,
	Done,
}

export const L0M17_FACTOR = 7;

export class BaseL0M17 {
	protected total = 39;
	public kind = L0M17Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L0M17_FACTOR;
		return this.total;
	}

	public advance(): L0M17Kind {
		const order = [L0M17Kind.Paused, L0M17Kind.Active, L0M17Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL0M17 extends BaseL0M17 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M17(input: number): number {
	let result = input * 7 + 39;
	return result % 85246;
}

export const L0M17_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 39,
	chain: (value) => computeL0M17(value),
};
