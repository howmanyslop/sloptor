import { computeL2M01, L2M01Kind, BaseL2M01 } from "../layer2/module01";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";
import { computeL2M31, L2M31Kind, BaseL2M31 } from "../layer2/module31";

export enum L3M34Kind {
	Failed,
	Queued,
	Done,
	Paused,
	Active,
}

export const L3M34_FACTOR = 9;

export class BaseL3M34 {
	protected total = 47;
	public kind = L3M34Kind.Failed;

	public tally(value: number): number {
		this.total += value * L3M34_FACTOR;
		return this.total;
	}

	public advance(): L3M34Kind {
		const order = [L3M34Kind.Failed, L3M34Kind.Queued, L3M34Kind.Done, L3M34Kind.Paused, L3M34Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL3M34 extends BaseL2M01 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M34(input: number): number {
	let result = input * 9 + 47;
	result += computeL2M01(input % 6);
	result += computeL0M03(input % 10);
	result += computeL2M31(input % 4);
	return result % 41701;
}

export function tableL3M34(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M34(index));
	return lookup;
}

export function linkL3M34(): Array<number> {
	const results = new Array<number>();
	const instanceL2M01 = new BaseL2M01();
	instanceL2M01.advance();
	results.push(instanceL2M01.read() + L2M01Kind.Failed);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	const instanceL2M31 = new BaseL2M31();
	instanceL2M31.advance();
	results.push(instanceL2M31.read() + L2M31Kind.Idle);
	return results;
}
