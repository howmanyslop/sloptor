import { computeL8M28, L8M28Kind, BaseL8M28 } from "../layer8/module28";
import { computeL4M11, L4M11Kind, BaseL4M11 } from "../layer4/module11";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L9M37Kind {
	Idle,
	Failed,
	Queued,
}

export const L9M37_FACTOR = 17;

export class BaseL9M37 {
	protected total = 18;
	public kind = L9M37Kind.Idle;

	public tally(value: number): number {
		this.total += value * L9M37_FACTOR;
		return this.total;
	}

	public advance(): L9M37Kind {
		const order = [L9M37Kind.Idle, L9M37Kind.Failed, L9M37Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M37(input: number): number {
	let result = input * 17 + 18;
	result += computeL8M28(input % 5);
	result += computeL4M11(input % 5);
	result += computeL0M03(input % 7);
	return result % 87236;
}

export function tableL9M37(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M37(index));
	return lookup;
}

export function linkL9M37(): Array<number> {
	const results = new Array<number>();
	const instanceL8M28 = new BaseL8M28();
	instanceL8M28.advance();
	results.push(instanceL8M28.read() + L8M28Kind.Done);
	const instanceL4M11 = new BaseL4M11();
	instanceL4M11.advance();
	results.push(instanceL4M11.read() + L4M11Kind.Done);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
