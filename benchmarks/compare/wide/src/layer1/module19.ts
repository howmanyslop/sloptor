import { computeL0M14, L0M14Kind, BaseL0M14 } from "../layer0/module14";

export enum L1M19Kind {
	Idle,
	Failed,
	Locked,
	Queued,
}

export const L1M19_FACTOR = 8;

export class BaseL1M19 {
	protected total = 38;
	public kind = L1M19Kind.Idle;

	public shift(value: number): number {
		this.total += value * L1M19_FACTOR;
		return this.total;
	}

	public advance(): L1M19Kind {
		const order = [L1M19Kind.Idle, L1M19Kind.Failed, L1M19Kind.Locked, L1M19Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL1M19 extends BaseL0M14 {
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

export function computeL1M19(input: number): number {
	let result = input * 8 + 38;
	result += computeL0M14(input % 9);
	return result % 8889;
}

export function tableL1M19(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M19(index));
	return lookup;
}

export function linkL1M19(): Array<number> {
	const results = new Array<number>();
	const instanceL0M14 = new BaseL0M14();
	instanceL0M14.advance();
	results.push(instanceL0M14.read() + L0M14Kind.Idle);
	return results;
}
