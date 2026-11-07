import { computeL8M33, L8M33Kind, BaseL8M33 } from "../layer8/module33";
import { computeL1M14, L1M14Kind, BaseL1M14 } from "../layer1/module14";

export enum L9M32Kind {
	Idle,
	Active,
	Queued,
	Paused,
	Locked,
}

export const L9M32_FACTOR = 8;

export class BaseL9M32 {
	protected total = 73;
	public kind = L9M32Kind.Idle;

	public weigh(value: number): number {
		this.total += value * L9M32_FACTOR;
		return this.total;
	}

	public advance(): L9M32Kind {
		const order = [L9M32Kind.Idle, L9M32Kind.Active, L9M32Kind.Queued, L9M32Kind.Paused, L9M32Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M32(input: number): number {
	let result = input * 8 + 73;
	result += computeL8M33(input % 10);
	result += computeL1M14(input % 5);
	return result % 37534;
}

export function tableL9M32(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M32(index));
	return lookup;
}

export function linkL9M32(): Array<number> {
	const results = new Array<number>();
	const instanceL8M33 = new BaseL8M33();
	instanceL8M33.advance();
	results.push(instanceL8M33.read() + L8M33Kind.Done);
	const instanceL1M14 = new BaseL1M14();
	instanceL1M14.advance();
	results.push(instanceL1M14.read() + L1M14Kind.Queued);
	return results;
}
