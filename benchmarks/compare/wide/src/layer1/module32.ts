import { computeL0M25, L0M25Kind, BaseL0M25 } from "../layer0/module25";

export enum L1M32Kind {
	Idle,
	Queued,
	Active,
	Paused,
	Failed,
}

export const L1M32_FACTOR = 3;

export class BaseL1M32 {
	protected total = 30;
	public kind = L1M32Kind.Idle;

	public fold(value: number): number {
		this.total += value * L1M32_FACTOR;
		return this.total;
	}

	public advance(): L1M32Kind {
		const order = [L1M32Kind.Idle, L1M32Kind.Queued, L1M32Kind.Active, L1M32Kind.Paused, L1M32Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL1M32 extends BaseL1M32 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 11) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M32(input: number): number {
	let result = input * 3 + 30;
	result += computeL0M25(input % 6);
	return result % 52706;
}

export const L1M32_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 30,
	chain: (value) => computeL1M32(value),
};

export function linkL1M32(): Array<number> {
	const results = new Array<number>();
	const instanceL0M25 = new BaseL0M25();
	instanceL0M25.advance();
	results.push(instanceL0M25.read() + L0M25Kind.Done);
	return results;
}
