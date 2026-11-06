import { computeL6M25, L6M25Kind, BaseL6M25 } from "../layer6/module25";
import { computeL6M13, L6M13Kind, BaseL6M13 } from "../layer6/module13";
import { computeL0M37, L0M37Kind, BaseL0M37 } from "../layer0/module37";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L7M14Kind {
	Idle,
	Locked,
	Done,
	Active,
}

export const L7M14_FACTOR = 15;

export class BaseL7M14 {
	protected total = 88;
	public kind = L7M14Kind.Idle;

	public fold(value: number): number {
		this.total += value * L7M14_FACTOR;
		return this.total;
	}

	public advance(): L7M14Kind {
		const order = [L7M14Kind.Idle, L7M14Kind.Locked, L7M14Kind.Done, L7M14Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M14(input: number): number {
	let result = input * 15 + 88;
	result += computeL6M25(input % 3);
	result += computeL6M13(input % 10);
	result += computeL0M37(input % 11);
	result += computeL0M02(input % 3);
	return result % 94655;
}

export function describeL7M14(kind: L7M14Kind): string {
	switch (kind) {
		case L7M14Kind.Idle:
			return "idle";
		case L7M14Kind.Locked:
			return "locked";
		case L7M14Kind.Done:
			return "done";
		case L7M14Kind.Active:
			return "active";
	}
}

export function linkL7M14(): Array<number> {
	const results = new Array<number>();
	const instanceL6M25 = new BaseL6M25();
	instanceL6M25.advance();
	results.push(instanceL6M25.read() + L6M25Kind.Paused);
	const instanceL6M13 = new BaseL6M13();
	instanceL6M13.advance();
	results.push(instanceL6M13.read() + L6M13Kind.Failed);
	const instanceL0M37 = new BaseL0M37();
	instanceL0M37.advance();
	results.push(instanceL0M37.read() + L0M37Kind.Done);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
