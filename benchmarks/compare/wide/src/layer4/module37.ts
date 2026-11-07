import { computeL3M23, L3M23Kind, BaseL3M23 } from "../layer3/module23";
import { computeL1M33, L1M33Kind, BaseL1M33 } from "../layer1/module33";
import { computeL1M01, L1M01Kind, BaseL1M01 } from "../layer1/module01";

export enum L4M37Kind {
	Queued,
	Idle,
	Done,
}

export const L4M37_FACTOR = 2;

export class BaseL4M37 {
	protected total = 57;
	public kind = L4M37Kind.Queued;

	public blend(value: number): number {
		this.total += value * L4M37_FACTOR;
		return this.total;
	}

	public advance(): L4M37Kind {
		const order = [L4M37Kind.Queued, L4M37Kind.Idle, L4M37Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL4M37 extends BaseL3M23 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 14) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M37(input: number): number {
	let result = input * 2 + 57;
	result += computeL3M23(input % 8);
	result += computeL1M33(input % 7);
	result += computeL1M01(input % 3);
	return result % 56670;
}

export function tableL4M37(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M37(index));
	return lookup;
}

export function linkL4M37(): Array<number> {
	const results = new Array<number>();
	const instanceL3M23 = new BaseL3M23();
	instanceL3M23.advance();
	results.push(instanceL3M23.read() + L3M23Kind.Queued);
	const instanceL1M33 = new BaseL1M33();
	instanceL1M33.advance();
	results.push(instanceL1M33.read() + L1M33Kind.Idle);
	const instanceL1M01 = new BaseL1M01();
	instanceL1M01.advance();
	results.push(instanceL1M01.read() + L1M01Kind.Locked);
	return results;
}
