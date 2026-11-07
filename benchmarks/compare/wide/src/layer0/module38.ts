export enum L0M38Kind {
	Idle,
	Active,
	Paused,
}

export const L0M38_FACTOR = 16;

export class BaseL0M38 {
	protected total = 84;
	public kind = L0M38Kind.Idle;

	public blend(value: number): number {
		this.total += value * L0M38_FACTOR;
		return this.total;
	}

	public advance(): L0M38Kind {
		const order = [L0M38Kind.Idle, L0M38Kind.Active, L0M38Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL0M38 extends BaseL0M38 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 16) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M38(input: number): number {
	let result = input * 16 + 84;
	return result % 71858;
}

export const L0M38_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 84,
	chain: (value) => computeL0M38(value),
};
