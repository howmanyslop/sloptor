export enum L0M14Kind {
	Idle,
	Locked,
	Active,
	Queued,
}

export const L0M14_FACTOR = 6;

export class BaseL0M14 {
	protected total = 51;
	public kind = L0M14Kind.Idle;

	public blend(value: number): number {
		this.total += value * L0M14_FACTOR;
		return this.total;
	}

	public advance(): L0M14Kind {
		const order = [L0M14Kind.Idle, L0M14Kind.Locked, L0M14Kind.Active, L0M14Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL0M14 extends BaseL0M14 {
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

export function computeL0M14(input: number): number {
	let result = input * 6 + 51;
	return result % 99203;
}

export const L0M14_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 51,
	chain: (value) => computeL0M14(value),
};
