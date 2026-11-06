import { computeL7M15, L7M15Kind, BaseL7M15 } from "../layer7/module15";
import { computeL5M21, L5M21Kind, BaseL5M21 } from "../layer5/module21";
import { computeL4M00, L4M00Kind, BaseL4M00 } from "../layer4/module00";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L8M01Kind {
	Idle,
	Queued,
	Done,
}

export const L8M01_FACTOR = 17;

export class BaseL8M01 {
	protected total = 7;
	public kind = L8M01Kind.Idle;

	public warp(value: number): number {
		this.total += value * L8M01_FACTOR;
		return this.total;
	}

	public advance(): L8M01Kind {
		const order = [L8M01Kind.Idle, L8M01Kind.Queued, L8M01Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M01(input: number): number {
	let result = input * 17 + 7;
	result += computeL7M15(input % 3);
	result += computeL5M21(input % 3);
	result += computeL4M00(input % 5);
	result += computeL0M03(input % 8);
	return result % 35632;
}

export function tableL8M01(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M01(index));
	return lookup;
}

export function linkL8M01(): Array<number> {
	const results = new Array<number>();
	const instanceL7M15 = new BaseL7M15();
	instanceL7M15.advance();
	results.push(instanceL7M15.read() + L7M15Kind.Failed);
	const instanceL5M21 = new BaseL5M21();
	instanceL5M21.advance();
	results.push(instanceL5M21.read() + L5M21Kind.Idle);
	const instanceL4M00 = new BaseL4M00();
	instanceL4M00.advance();
	results.push(instanceL4M00.read() + L4M00Kind.Idle);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
