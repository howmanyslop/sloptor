import { computeL8M11, L8M11Kind, BaseL8M11 } from "../layer8/module11";
import { computeL2M39, L2M39Kind, BaseL2M39 } from "../layer2/module39";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L9M06Kind {
	Queued,
	Locked,
	Failed,
	Done,
}

export const L9M06_FACTOR = 12;

export class BaseL9M06 {
	protected total = 78;
	public kind = L9M06Kind.Queued;

	public weigh(value: number): number {
		this.total += value * L9M06_FACTOR;
		return this.total;
	}

	public advance(): L9M06Kind {
		const order = [L9M06Kind.Queued, L9M06Kind.Locked, L9M06Kind.Failed, L9M06Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M06(input: number): number {
	let result = input * 12 + 78;
	result += computeL8M11(input % 9);
	result += computeL2M39(input % 3);
	result += computeL0M02(input % 10);
	return result % 64001;
}

export function tableL9M06(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M06(index));
	return lookup;
}

export function linkL9M06(): Array<number> {
	const results = new Array<number>();
	const instanceL8M11 = new BaseL8M11();
	instanceL8M11.advance();
	results.push(instanceL8M11.read() + L8M11Kind.Failed);
	const instanceL2M39 = new BaseL2M39();
	instanceL2M39.advance();
	results.push(instanceL2M39.read() + L2M39Kind.Failed);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
