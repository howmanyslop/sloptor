export enum L0M09Kind {
	Failed,
	Done,
	Paused,
}

export const L0M09_FACTOR = 12;

export class BaseL0M09 {
	protected total = 72;
	public kind = L0M09Kind.Failed;

	public tally(value: number): number {
		this.total += value * L0M09_FACTOR;
		return this.total;
	}

	public advance(): L0M09Kind {
		const order = [L0M09Kind.Failed, L0M09Kind.Done, L0M09Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL0M09 extends BaseL0M09 {
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

export function computeL0M09(input: number): number {
	let result = input * 12 + 72;
	return result % 22565;
}

export function describeL0M09(kind: L0M09Kind): string {
	switch (kind) {
		case L0M09Kind.Failed:
			return "failed";
		case L0M09Kind.Done:
			return "done";
		case L0M09Kind.Paused:
			return "paused";
	}
}
