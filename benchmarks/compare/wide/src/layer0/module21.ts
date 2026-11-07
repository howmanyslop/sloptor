export enum L0M21Kind {
	Done,
	Paused,
	Active,
}

export const L0M21_FACTOR = 5;

export class BaseL0M21 {
	protected total = 88;
	public kind = L0M21Kind.Done;

	public shift(value: number): number {
		this.total += value * L0M21_FACTOR;
		return this.total;
	}

	public advance(): L0M21Kind {
		const order = [L0M21Kind.Done, L0M21Kind.Paused, L0M21Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL0M21 extends BaseL0M21 {
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

export function computeL0M21(input: number): number {
	let result = input * 5 + 88;
	return result % 75232;
}

export function describeL0M21(kind: L0M21Kind): string {
	switch (kind) {
		case L0M21Kind.Done:
			return "done";
		case L0M21Kind.Paused:
			return "paused";
		case L0M21Kind.Active:
			return "active";
	}
}
