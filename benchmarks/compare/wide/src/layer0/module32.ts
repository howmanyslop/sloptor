export enum L0M32Kind {
	Paused,
	Locked,
	Active,
	Failed,
	Idle,
}

export const L0M32_FACTOR = 8;

export class BaseL0M32 {
	protected total = 28;
	public kind = L0M32Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L0M32_FACTOR;
		return this.total;
	}

	public advance(): L0M32Kind {
		const order = [L0M32Kind.Paused, L0M32Kind.Locked, L0M32Kind.Active, L0M32Kind.Failed, L0M32Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL0M32 extends BaseL0M32 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M32(input: number): number {
	let result = input * 8 + 28;
	return result % 42962;
}

export const L0M32_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 28,
	chain: (value) => computeL0M32(value),
};
