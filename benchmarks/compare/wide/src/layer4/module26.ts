import { computeL3M26, L3M26Kind, BaseL3M26 } from "../layer3/module26";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L4M26Kind {
	Active,
	Failed,
	Idle,
	Locked,
	Done,
}

export const L4M26_FACTOR = 4;

export class BaseL4M26 {
	protected total = 21;
	public kind = L4M26Kind.Active;

	public tally(value: number): number {
		this.total += value * L4M26_FACTOR;
		return this.total;
	}

	public advance(): L4M26Kind {
		const order = [L4M26Kind.Active, L4M26Kind.Failed, L4M26Kind.Idle, L4M26Kind.Locked, L4M26Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M26(input: number): number {
	let result = input * 4 + 21;
	result += computeL3M26(input % 4);
	result += computeL0M00(input % 9);
	return result % 54563;
}

export function tableL4M26(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M26(index));
	return lookup;
}

export function linkL4M26(): Array<number> {
	const results = new Array<number>();
	const instanceL3M26 = new BaseL3M26();
	instanceL3M26.advance();
	results.push(instanceL3M26.read() + L3M26Kind.Failed);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
