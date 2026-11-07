export enum L0M11Kind {
	Active,
	Locked,
	Idle,
	Queued,
	Paused,
}

export const L0M11_FACTOR = 6;

export class BaseL0M11 {
	protected total = 19;
	public kind = L0M11Kind.Active;

	public shift(value: number): number {
		this.total += value * L0M11_FACTOR;
		return this.total;
	}

	public advance(): L0M11Kind {
		const order = [L0M11Kind.Active, L0M11Kind.Locked, L0M11Kind.Idle, L0M11Kind.Queued, L0M11Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL0M11 extends BaseL0M11 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M11(input: number): number {
	let result = input * 6 + 19;
	return result % 29382;
}

export function describeL0M11(kind: L0M11Kind): string {
	switch (kind) {
		case L0M11Kind.Active:
			return "active";
		case L0M11Kind.Locked:
			return "locked";
		case L0M11Kind.Idle:
			return "idle";
		case L0M11Kind.Queued:
			return "queued";
		case L0M11Kind.Paused:
			return "paused";
	}
}
