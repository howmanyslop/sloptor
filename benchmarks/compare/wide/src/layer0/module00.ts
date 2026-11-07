export enum L0M00Kind {
	Done,
	Paused,
	Failed,
	Active,
}

export const L0M00_FACTOR = 19;

export class BaseL0M00 {
	protected total = 27;
	public kind = L0M00Kind.Done;

	public weigh(value: number): number {
		this.total += value * L0M00_FACTOR;
		return this.total;
	}

	public advance(): L0M00Kind {
		const order = [L0M00Kind.Done, L0M00Kind.Paused, L0M00Kind.Failed, L0M00Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL0M00 extends BaseL0M00 {
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

export function computeL0M00(input: number): number {
	let result = input * 19 + 27;
	return result % 86634;
}

export function describeL0M00(kind: L0M00Kind): string {
	switch (kind) {
		case L0M00Kind.Done:
			return "done";
		case L0M00Kind.Paused:
			return "paused";
		case L0M00Kind.Failed:
			return "failed";
		case L0M00Kind.Active:
			return "active";
	}
}
