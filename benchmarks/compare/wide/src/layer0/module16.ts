export enum L0M16Kind {
	Paused,
	Active,
	Locked,
	Done,
}

export const L0M16_FACTOR = 15;

export class BaseL0M16 {
	protected total = 55;
	public kind = L0M16Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L0M16_FACTOR;
		return this.total;
	}

	public advance(): L0M16Kind {
		const order = [L0M16Kind.Paused, L0M16Kind.Active, L0M16Kind.Locked, L0M16Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL0M16 extends BaseL0M16 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M16(input: number): number {
	let result = input * 15 + 55;
	return result % 62683;
}

export const L0M16_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 55,
	chain: (value) => computeL0M16(value),
};
