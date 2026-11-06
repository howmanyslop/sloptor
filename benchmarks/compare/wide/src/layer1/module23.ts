import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L1M23Kind {
	Idle,
	Active,
	Locked,
}

export const L1M23_FACTOR = 13;

export class BaseL1M23 {
	protected total = 90;
	public kind = L1M23Kind.Idle;

	public weigh(value: number): number {
		this.total += value * L1M23_FACTOR;
		return this.total;
	}

	public advance(): L1M23Kind {
		const order = [L1M23Kind.Idle, L1M23Kind.Active, L1M23Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL1M23 extends BaseL0M02 {
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

export function computeL1M23(input: number): number {
	let result = input * 13 + 90;
	result += computeL0M02(input % 7);
	return result % 61044;
}

export function tableL1M23(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M23(index));
	return lookup;
}

export function linkL1M23(): Array<number> {
	const results = new Array<number>();
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
