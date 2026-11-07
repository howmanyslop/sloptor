import { computeL4M26, L4M26Kind, BaseL4M26 } from "../layer4/module26";
import { computeL2M09, L2M09Kind, BaseL2M09 } from "../layer2/module09";

export enum L5M03Kind {
	Queued,
	Idle,
	Paused,
}

export const L5M03_FACTOR = 10;

export class BaseL5M03 {
	protected total = 88;
	public kind = L5M03Kind.Queued;

	public scale(value: number): number {
		this.total += value * L5M03_FACTOR;
		return this.total;
	}

	public advance(): L5M03Kind {
		const order = [L5M03Kind.Queued, L5M03Kind.Idle, L5M03Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M03(input: number): number {
	let result = input * 10 + 88;
	result += computeL4M26(input % 5);
	result += computeL2M09(input % 10);
	return result % 8584;
}

export function tableL5M03(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M03(index));
	return lookup;
}

export function linkL5M03(): Array<number> {
	const results = new Array<number>();
	const instanceL4M26 = new BaseL4M26();
	instanceL4M26.advance();
	results.push(instanceL4M26.read() + L4M26Kind.Active);
	const instanceL2M09 = new BaseL2M09();
	instanceL2M09.advance();
	results.push(instanceL2M09.read() + L2M09Kind.Done);
	return results;
}
