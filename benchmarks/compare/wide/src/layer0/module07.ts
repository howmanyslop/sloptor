export enum L0M07Kind {
	Idle,
	Locked,
	Done,
	Queued,
	Failed,
}

export const L0M07_FACTOR = 6;

export class BaseL0M07 {
	protected total = 20;
	public kind = L0M07Kind.Idle;

	public scale(value: number): number {
		this.total += value * L0M07_FACTOR;
		return this.total;
	}

	public advance(): L0M07Kind {
		const order = [L0M07Kind.Idle, L0M07Kind.Locked, L0M07Kind.Done, L0M07Kind.Queued, L0M07Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M07(input: number): number {
	let result = input * 6 + 20;
	return result % 98647;
}

export function describeL0M07(kind: L0M07Kind): string {
	switch (kind) {
		case L0M07Kind.Idle:
			return "idle";
		case L0M07Kind.Locked:
			return "locked";
		case L0M07Kind.Done:
			return "done";
		case L0M07Kind.Queued:
			return "queued";
		case L0M07Kind.Failed:
			return "failed";
	}
}
