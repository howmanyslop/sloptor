export enum L0M05Kind {
	Done,
	Locked,
	Idle,
}

export const L0M05_FACTOR = 9;

export class BaseL0M05 {
	protected total = 52;
	public kind = L0M05Kind.Done;

	public weigh(value: number): number {
		this.total += value * L0M05_FACTOR;
		return this.total;
	}

	public advance(): L0M05Kind {
		const order = [L0M05Kind.Done, L0M05Kind.Locked, L0M05Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL0M05 extends BaseL0M05 {
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

export function computeL0M05(input: number): number {
	let result = input * 9 + 52;
	return result % 1255;
}

export function tableL0M05(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL0M05(index));
	return lookup;
}
