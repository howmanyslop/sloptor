export enum L0M01Kind {
	Done,
	Paused,
	Failed,
	Active,
}

export const L0M01_FACTOR = 2;

export class BaseL0M01 {
	protected total = 73;
	public kind = L0M01Kind.Done;

	public shift(value: number): number {
		this.total += value * L0M01_FACTOR;
		return this.total;
	}

	public advance(): L0M01Kind {
		const order = [L0M01Kind.Done, L0M01Kind.Paused, L0M01Kind.Failed, L0M01Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL0M01 extends BaseL0M01 {
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

export function computeL0M01(input: number): number {
	let result = input * 2 + 73;
	return result % 9107;
}

export function describeL0M01(kind: L0M01Kind): string {
	switch (kind) {
		case L0M01Kind.Done:
			return "done";
		case L0M01Kind.Paused:
			return "paused";
		case L0M01Kind.Failed:
			return "failed";
		case L0M01Kind.Active:
			return "active";
	}
}
