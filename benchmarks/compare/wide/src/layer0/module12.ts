export enum L0M12Kind {
	Done,
	Paused,
	Locked,
	Queued,
}

export const L0M12_FACTOR = 17;

export class BaseL0M12 {
	protected total = 60;
	public kind = L0M12Kind.Done;

	public shift(value: number): number {
		this.total += value * L0M12_FACTOR;
		return this.total;
	}

	public advance(): L0M12Kind {
		const order = [L0M12Kind.Done, L0M12Kind.Paused, L0M12Kind.Locked, L0M12Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL0M12 extends BaseL0M12 {
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

export function computeL0M12(input: number): number {
	let result = input * 17 + 60;
	return result % 10854;
}

export const L0M12_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 60,
	chain: (value) => computeL0M12(value),
};
