import { computeL1M21, L1M21Kind, BaseL1M21 } from "../layer1/module21";
import { computeL0M32, L0M32Kind, BaseL0M32 } from "../layer0/module32";
import { computeL1M01, L1M01Kind, BaseL1M01 } from "../layer1/module01";

export enum L2M37Kind {
	Failed,
	Done,
	Queued,
	Paused,
	Locked,
}

export const L2M37_FACTOR = 2;

export class BaseL2M37 {
	protected total = 50;
	public kind = L2M37Kind.Failed;

	public blend(value: number): number {
		this.total += value * L2M37_FACTOR;
		return this.total;
	}

	public advance(): L2M37Kind {
		const order = [L2M37Kind.Failed, L2M37Kind.Done, L2M37Kind.Queued, L2M37Kind.Paused, L2M37Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL2M37 extends BaseL1M21 {
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

export function computeL2M37(input: number): number {
	let result = input * 2 + 50;
	result += computeL1M21(input % 11);
	result += computeL0M32(input % 3);
	result += computeL1M01(input % 4);
	return result % 34067;
}

export function tableL2M37(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL2M37(index));
	return lookup;
}

export function linkL2M37(): Array<number> {
	const results = new Array<number>();
	const instanceL1M21 = new BaseL1M21();
	instanceL1M21.advance();
	results.push(instanceL1M21.read() + L1M21Kind.Failed);
	const instanceL0M32 = new BaseL0M32();
	instanceL0M32.advance();
	results.push(instanceL0M32.read() + L0M32Kind.Paused);
	const instanceL1M01 = new BaseL1M01();
	instanceL1M01.advance();
	results.push(instanceL1M01.read() + L1M01Kind.Locked);
	return results;
}
