export enum L0M29Kind {
	Paused,
	Locked,
	Done,
	Active,
}

export const L0M29_FACTOR = 13;

export class BaseL0M29 {
	protected total = 65;
	public kind = L0M29Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L0M29_FACTOR;
		return this.total;
	}

	public advance(): L0M29Kind {
		const order = [L0M29Kind.Paused, L0M29Kind.Locked, L0M29Kind.Done, L0M29Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL0M29 extends BaseL0M29 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 9) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M29(input: number): number {
	let result = input * 13 + 65;
	return result % 22189;
}

export function describeL0M29(kind: L0M29Kind): string {
	switch (kind) {
		case L0M29Kind.Paused:
			return "paused";
		case L0M29Kind.Locked:
			return "locked";
		case L0M29Kind.Done:
			return "done";
		case L0M29Kind.Active:
			return "active";
	}
}
