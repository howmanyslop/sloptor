import { computeL8M17, L8M17Kind, BaseL8M17 } from "../layer8/module17";
import { computeL4M23, L4M23Kind, BaseL4M23 } from "../layer4/module23";
import { computeL5M06, L5M06Kind, BaseL5M06 } from "../layer5/module06";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L9M24Kind {
	Idle,
	Paused,
	Locked,
}

export const L9M24_FACTOR = 6;

export class BaseL9M24 {
	protected total = 16;
	public kind = L9M24Kind.Idle;

	public mix(value: number): number {
		this.total += value * L9M24_FACTOR;
		return this.total;
	}

	public advance(): L9M24Kind {
		const order = [L9M24Kind.Idle, L9M24Kind.Paused, L9M24Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M24(input: number): number {
	let result = input * 6 + 16;
	result += computeL8M17(input % 5);
	result += computeL4M23(input % 4);
	result += computeL5M06(input % 11);
	result += computeL0M01(input % 10);
	return result % 43728;
}

export function describeL9M24(kind: L9M24Kind): string {
	switch (kind) {
		case L9M24Kind.Idle:
			return "idle";
		case L9M24Kind.Paused:
			return "paused";
		case L9M24Kind.Locked:
			return "locked";
	}
}

export function linkL9M24(): Array<number> {
	const results = new Array<number>();
	const instanceL8M17 = new BaseL8M17();
	instanceL8M17.advance();
	results.push(instanceL8M17.read() + L8M17Kind.Idle);
	const instanceL4M23 = new BaseL4M23();
	instanceL4M23.advance();
	results.push(instanceL4M23.read() + L4M23Kind.Active);
	const instanceL5M06 = new BaseL5M06();
	instanceL5M06.advance();
	results.push(instanceL5M06.read() + L5M06Kind.Done);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
