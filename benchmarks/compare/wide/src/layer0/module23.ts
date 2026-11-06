export enum L0M23Kind {
	Idle,
	Active,
	Paused,
	Locked,
}

export const L0M23_FACTOR = 16;

export class BaseL0M23 {
	protected total = 56;
	public kind = L0M23Kind.Idle;

	public mix(value: number): number {
		this.total += value * L0M23_FACTOR;
		return this.total;
	}

	public advance(): L0M23Kind {
		const order = [L0M23Kind.Idle, L0M23Kind.Active, L0M23Kind.Paused, L0M23Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M23(input: number): number {
	let result = input * 16 + 56;
	return result % 26962;
}

export function describeL0M23(kind: L0M23Kind): string {
	switch (kind) {
		case L0M23Kind.Idle:
			return "idle";
		case L0M23Kind.Active:
			return "active";
		case L0M23Kind.Paused:
			return "paused";
		case L0M23Kind.Locked:
			return "locked";
	}
}
