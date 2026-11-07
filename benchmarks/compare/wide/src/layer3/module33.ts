import { computeL2M34, L2M34Kind, BaseL2M34 } from "../layer2/module34";
import { computeL1M13, L1M13Kind, BaseL1M13 } from "../layer1/module13";
import { computeL1M23, L1M23Kind, BaseL1M23 } from "../layer1/module23";

export enum L3M33Kind {
	Done,
	Queued,
	Paused,
}

export const L3M33_FACTOR = 18;

export class BaseL3M33 {
	protected total = 71;
	public kind = L3M33Kind.Done;

	public scale(value: number): number {
		this.total += value * L3M33_FACTOR;
		return this.total;
	}

	public advance(): L3M33Kind {
		const order = [L3M33Kind.Done, L3M33Kind.Queued, L3M33Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL3M33 extends BaseL2M34 {
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

export function computeL3M33(input: number): number {
	let result = input * 18 + 71;
	result += computeL2M34(input % 3);
	result += computeL1M13(input % 11);
	result += computeL1M23(input % 6);
	return result % 96097;
}

export function tableL3M33(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M33(index));
	return lookup;
}

export function linkL3M33(): Array<number> {
	const results = new Array<number>();
	const instanceL2M34 = new BaseL2M34();
	instanceL2M34.advance();
	results.push(instanceL2M34.read() + L2M34Kind.Failed);
	const instanceL1M13 = new BaseL1M13();
	instanceL1M13.advance();
	results.push(instanceL1M13.read() + L1M13Kind.Idle);
	const instanceL1M23 = new BaseL1M23();
	instanceL1M23.advance();
	results.push(instanceL1M23.read() + L1M23Kind.Idle);
	return results;
}
