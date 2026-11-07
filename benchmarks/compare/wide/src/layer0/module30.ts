export enum L0M30Kind {
	Done,
	Locked,
	Queued,
}

export const L0M30_FACTOR = 15;

export class BaseL0M30 {
	protected total = 20;
	public kind = L0M30Kind.Done;

	public blend(value: number): number {
		this.total += value * L0M30_FACTOR;
		return this.total;
	}

	public advance(): L0M30Kind {
		const order = [L0M30Kind.Done, L0M30Kind.Locked, L0M30Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL0M30 extends BaseL0M30 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL0M30(input: number): number {
	let result = input * 15 + 20;
	return result % 60939;
}

export function describeL0M30(kind: L0M30Kind): string {
	switch (kind) {
		case L0M30Kind.Done:
			return "done";
		case L0M30Kind.Locked:
			return "locked";
		case L0M30Kind.Queued:
			return "queued";
	}
}
