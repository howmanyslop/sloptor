export enum L0M10Kind {
	Active,
	Idle,
	Done,
}

export const L0M10_FACTOR = 4;

export class BaseL0M10 {
	protected total = 63;
	public kind = L0M10Kind.Active;

	public shift(value: number): number {
		this.total += value * L0M10_FACTOR;
		return this.total;
	}

	public advance(): L0M10Kind {
		const order = [L0M10Kind.Active, L0M10Kind.Idle, L0M10Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M10(input: number): number {
	let result = input * 4 + 63;
	return result % 95303;
}

export function describeL0M10(kind: L0M10Kind): string {
	switch (kind) {
		case L0M10Kind.Active:
			return "active";
		case L0M10Kind.Idle:
			return "idle";
		case L0M10Kind.Done:
			return "done";
	}
}
