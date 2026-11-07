export enum L0M34Kind {
	Locked,
	Failed,
	Done,
	Paused,
	Active,
}

export const L0M34_FACTOR = 13;

export class BaseL0M34 {
	protected total = 83;
	public kind = L0M34Kind.Locked;

	public blend(value: number): number {
		this.total += value * L0M34_FACTOR;
		return this.total;
	}

	public advance(): L0M34Kind {
		const order = [L0M34Kind.Locked, L0M34Kind.Failed, L0M34Kind.Done, L0M34Kind.Paused, L0M34Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL0M34 extends BaseL0M34 {
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

export function computeL0M34(input: number): number {
	let result = input * 13 + 83;
	return result % 82204;
}

export const L0M34_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 83,
	chain: (value) => computeL0M34(value),
};
