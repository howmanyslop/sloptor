import { computeL5M29, L5M29Kind, BaseL5M29 } from "../layer5/module29";
import { computeL2M16, L2M16Kind, BaseL2M16 } from "../layer2/module16";

export enum L6M09Kind {
	Queued,
	Locked,
	Idle,
}

export const L6M09_FACTOR = 13;

export class BaseL6M09 {
	protected total = 92;
	public kind = L6M09Kind.Queued;

	public mix(value: number): number {
		this.total += value * L6M09_FACTOR;
		return this.total;
	}

	public advance(): L6M09Kind {
		const order = [L6M09Kind.Queued, L6M09Kind.Locked, L6M09Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M09(input: number): number {
	let result = input * 13 + 92;
	result += computeL5M29(input % 4);
	result += computeL2M16(input % 8);
	return result % 45568;
}

export function tableL6M09(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M09(index));
	return lookup;
}

export function linkL6M09(): Array<number> {
	const results = new Array<number>();
	const instanceL5M29 = new BaseL5M29();
	instanceL5M29.advance();
	results.push(instanceL5M29.read() + L5M29Kind.Paused);
	const instanceL2M16 = new BaseL2M16();
	instanceL2M16.advance();
	results.push(instanceL2M16.read() + L2M16Kind.Paused);
	return results;
}
