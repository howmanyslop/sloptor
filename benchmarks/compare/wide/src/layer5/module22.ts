import { computeL4M39, L4M39Kind, BaseL4M39 } from "../layer4/module39";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";
import { computeL3M30, L3M30Kind, BaseL3M30 } from "../layer3/module30";

export enum L5M22Kind {
	Queued,
	Locked,
	Idle,
}

export const L5M22_FACTOR = 3;

export class BaseL5M22 {
	protected total = 95;
	public kind = L5M22Kind.Queued;

	public warp(value: number): number {
		this.total += value * L5M22_FACTOR;
		return this.total;
	}

	public advance(): L5M22Kind {
		const order = [L5M22Kind.Queued, L5M22Kind.Locked, L5M22Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M22(input: number): number {
	let result = input * 3 + 95;
	result += computeL4M39(input % 4);
	result += computeL0M01(input % 7);
	result += computeL3M30(input % 4);
	return result % 20910;
}

export const L5M22_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 95,
	chain: (value) => computeL5M22(value),
};

export function linkL5M22(): Array<number> {
	const results = new Array<number>();
	const instanceL4M39 = new BaseL4M39();
	instanceL4M39.advance();
	results.push(instanceL4M39.read() + L4M39Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	const instanceL3M30 = new BaseL3M30();
	instanceL3M30.advance();
	results.push(instanceL3M30.read() + L3M30Kind.Done);
	return results;
}
