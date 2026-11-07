export enum L0M27Kind {
	Done,
	Idle,
	Active,
	Queued,
	Failed,
}

export const L0M27_FACTOR = 9;

export class BaseL0M27 {
	protected total = 5;
	public kind = L0M27Kind.Done;

	public fold(value: number): number {
		this.total += value * L0M27_FACTOR;
		return this.total;
	}

	public advance(): L0M27Kind {
		const order = [L0M27Kind.Done, L0M27Kind.Idle, L0M27Kind.Active, L0M27Kind.Queued, L0M27Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL0M27(input: number): number {
	let result = input * 9 + 5;
	return result % 86270;
}

export function describeL0M27(kind: L0M27Kind): string {
	switch (kind) {
		case L0M27Kind.Done:
			return "done";
		case L0M27Kind.Idle:
			return "idle";
		case L0M27Kind.Active:
			return "active";
		case L0M27Kind.Queued:
			return "queued";
		case L0M27Kind.Failed:
			return "failed";
	}
}
