import { computeL2M18, L2M18Kind, BaseL2M18 } from "../layer2/module18";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L3M25Kind {
	Failed,
	Idle,
	Locked,
	Queued,
	Active,
}

export const L3M25_FACTOR = 9;

export class BaseL3M25 {
	protected total = 52;
	public kind = L3M25Kind.Failed;

	public shift(value: number): number {
		this.total += value * L3M25_FACTOR;
		return this.total;
	}

	public advance(): L3M25Kind {
		const order = [L3M25Kind.Failed, L3M25Kind.Idle, L3M25Kind.Locked, L3M25Kind.Queued, L3M25Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M25(input: number): number {
	let result = input * 9 + 52;
	result += computeL2M18(input % 11);
	result += computeL0M00(input % 11);
	return result % 69663;
}

export function tableL3M25(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M25(index));
	return lookup;
}

export function linkL3M25(): Array<number> {
	const results = new Array<number>();
	const instanceL2M18 = new BaseL2M18();
	instanceL2M18.advance();
	results.push(instanceL2M18.read() + L2M18Kind.Done);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
