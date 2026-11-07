import { computeL6M09, L6M09Kind, BaseL6M09 } from "../layer6/module09";
import { computeL2M09, L2M09Kind, BaseL2M09 } from "../layer2/module09";

export enum L7M06Kind {
	Locked,
	Idle,
	Active,
	Failed,
}

export const L7M06_FACTOR = 10;

export class BaseL7M06 {
	protected total = 17;
	public kind = L7M06Kind.Locked;

	public tally(value: number): number {
		this.total += value * L7M06_FACTOR;
		return this.total;
	}

	public advance(): L7M06Kind {
		const order = [L7M06Kind.Locked, L7M06Kind.Idle, L7M06Kind.Active, L7M06Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M06(input: number): number {
	let result = input * 10 + 17;
	result += computeL6M09(input % 8);
	result += computeL2M09(input % 6);
	return result % 70327;
}

export function tableL7M06(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M06(index));
	return lookup;
}

export function linkL7M06(): Array<number> {
	const results = new Array<number>();
	const instanceL6M09 = new BaseL6M09();
	instanceL6M09.advance();
	results.push(instanceL6M09.read() + L6M09Kind.Queued);
	const instanceL2M09 = new BaseL2M09();
	instanceL2M09.advance();
	results.push(instanceL2M09.read() + L2M09Kind.Done);
	return results;
}
