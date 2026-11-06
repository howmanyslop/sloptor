import { computeL3M11, L3M11Kind, BaseL3M11 } from "../layer3/module11";
import { computeL0M37, L0M37Kind, BaseL0M37 } from "../layer0/module37";
import { computeL3M26, L3M26Kind, BaseL3M26 } from "../layer3/module26";

export enum L4M01Kind {
	Locked,
	Done,
	Idle,
}

export const L4M01_FACTOR = 16;

export class BaseL4M01 {
	protected total = 78;
	public kind = L4M01Kind.Locked;

	public scale(value: number): number {
		this.total += value * L4M01_FACTOR;
		return this.total;
	}

	public advance(): L4M01Kind {
		const order = [L4M01Kind.Locked, L4M01Kind.Done, L4M01Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL4M01 extends BaseL4M01 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 16) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M01(input: number): number {
	let result = input * 16 + 78;
	result += computeL3M11(input % 11);
	result += computeL0M37(input % 8);
	result += computeL3M26(input % 9);
	return result % 57983;
}

export function tableL4M01(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M01(index));
	return lookup;
}

export function linkL4M01(): Array<number> {
	const results = new Array<number>();
	const instanceL3M11 = new BaseL3M11();
	instanceL3M11.advance();
	results.push(instanceL3M11.read() + L3M11Kind.Idle);
	const instanceL0M37 = new BaseL0M37();
	instanceL0M37.advance();
	results.push(instanceL0M37.read() + L0M37Kind.Done);
	const instanceL3M26 = new BaseL3M26();
	instanceL3M26.advance();
	results.push(instanceL3M26.read() + L3M26Kind.Failed);
	return results;
}
