import { computeL4M17, L4M17Kind, BaseL4M17 } from "../layer4/module17";
import { computeL3M06, L3M06Kind, BaseL3M06 } from "../layer3/module06";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L5M36Kind {
	Idle,
	Queued,
	Active,
	Locked,
}

export const L5M36_FACTOR = 13;

export class BaseL5M36 {
	protected total = 31;
	public kind = L5M36Kind.Idle;

	public scale(value: number): number {
		this.total += value * L5M36_FACTOR;
		return this.total;
	}

	public advance(): L5M36Kind {
		const order = [L5M36Kind.Idle, L5M36Kind.Queued, L5M36Kind.Active, L5M36Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M36(input: number): number {
	let result = input * 13 + 31;
	result += computeL4M17(input % 9);
	result += computeL3M06(input % 5);
	result += computeL0M03(input % 4);
	return result % 72460;
}

export function tableL5M36(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M36(index));
	return lookup;
}

export function linkL5M36(): Array<number> {
	const results = new Array<number>();
	const instanceL4M17 = new BaseL4M17();
	instanceL4M17.advance();
	results.push(instanceL4M17.read() + L4M17Kind.Done);
	const instanceL3M06 = new BaseL3M06();
	instanceL3M06.advance();
	results.push(instanceL3M06.read() + L3M06Kind.Done);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
