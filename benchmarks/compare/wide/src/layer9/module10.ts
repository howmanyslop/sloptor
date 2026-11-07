import { computeL8M39, L8M39Kind, BaseL8M39 } from "../layer8/module39";
import { computeL8M27, L8M27Kind, BaseL8M27 } from "../layer8/module27";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L9M10Kind {
	Done,
	Active,
	Locked,
	Failed,
	Paused,
}

export const L9M10_FACTOR = 18;

export class BaseL9M10 {
	protected total = 47;
	public kind = L9M10Kind.Done;

	public warp(value: number): number {
		this.total += value * L9M10_FACTOR;
		return this.total;
	}

	public advance(): L9M10Kind {
		const order = [L9M10Kind.Done, L9M10Kind.Active, L9M10Kind.Locked, L9M10Kind.Failed, L9M10Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M10(input: number): number {
	let result = input * 18 + 47;
	result += computeL8M39(input % 4);
	result += computeL8M27(input % 8);
	result += computeL0M01(input % 5);
	return result % 72792;
}

export function describeL9M10(kind: L9M10Kind): string {
	switch (kind) {
		case L9M10Kind.Done:
			return "done";
		case L9M10Kind.Active:
			return "active";
		case L9M10Kind.Locked:
			return "locked";
		case L9M10Kind.Failed:
			return "failed";
		case L9M10Kind.Paused:
			return "paused";
	}
}

export function linkL9M10(): Array<number> {
	const results = new Array<number>();
	const instanceL8M39 = new BaseL8M39();
	instanceL8M39.advance();
	results.push(instanceL8M39.read() + L8M39Kind.Done);
	const instanceL8M27 = new BaseL8M27();
	instanceL8M27.advance();
	results.push(instanceL8M27.read() + L8M27Kind.Paused);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
