import { computeL7M05, L7M05Kind, BaseL7M05 } from "../layer7/module05";
import { computeL5M16, L5M16Kind, BaseL5M16 } from "../layer5/module16";
import { computeL3M16, L3M16Kind, BaseL3M16 } from "../layer3/module16";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L8M11Kind {
	Failed,
	Locked,
	Paused,
	Done,
}

export const L8M11_FACTOR = 6;

export class BaseL8M11 {
	protected total = 2;
	public kind = L8M11Kind.Failed;

	public fold(value: number): number {
		this.total += value * L8M11_FACTOR;
		return this.total;
	}

	public advance(): L8M11Kind {
		const order = [L8M11Kind.Failed, L8M11Kind.Locked, L8M11Kind.Paused, L8M11Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M11(input: number): number {
	let result = input * 6 + 2;
	result += computeL7M05(input % 6);
	result += computeL5M16(input % 5);
	result += computeL3M16(input % 11);
	result += computeL0M01(input % 11);
	return result % 38312;
}

export function describeL8M11(kind: L8M11Kind): string {
	switch (kind) {
		case L8M11Kind.Failed:
			return "failed";
		case L8M11Kind.Locked:
			return "locked";
		case L8M11Kind.Paused:
			return "paused";
		case L8M11Kind.Done:
			return "done";
	}
}

export function linkL8M11(): Array<number> {
	const results = new Array<number>();
	const instanceL7M05 = new BaseL7M05();
	instanceL7M05.advance();
	results.push(instanceL7M05.read() + L7M05Kind.Failed);
	const instanceL5M16 = new BaseL5M16();
	instanceL5M16.advance();
	results.push(instanceL5M16.read() + L5M16Kind.Done);
	const instanceL3M16 = new BaseL3M16();
	instanceL3M16.advance();
	results.push(instanceL3M16.read() + L3M16Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
