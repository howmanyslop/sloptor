import { computeL4M27, L4M27Kind, BaseL4M27 } from "../layer4/module27";
import { computeL3M07, L3M07Kind, BaseL3M07 } from "../layer3/module07";
import { computeL0M05, L0M05Kind, BaseL0M05 } from "../layer0/module05";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L5M26Kind {
	Locked,
	Paused,
	Queued,
}

export const L5M26_FACTOR = 9;

export class BaseL5M26 {
	protected total = 69;
	public kind = L5M26Kind.Locked;

	public scale(value: number): number {
		this.total += value * L5M26_FACTOR;
		return this.total;
	}

	public advance(): L5M26Kind {
		const order = [L5M26Kind.Locked, L5M26Kind.Paused, L5M26Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M26(input: number): number {
	let result = input * 9 + 69;
	result += computeL4M27(input % 8);
	result += computeL3M07(input % 8);
	result += computeL0M05(input % 3);
	result += computeL0M02(input % 4);
	return result % 9192;
}

export function describeL5M26(kind: L5M26Kind): string {
	switch (kind) {
		case L5M26Kind.Locked:
			return "locked";
		case L5M26Kind.Paused:
			return "paused";
		case L5M26Kind.Queued:
			return "queued";
	}
}

export function linkL5M26(): Array<number> {
	const results = new Array<number>();
	const instanceL4M27 = new BaseL4M27();
	instanceL4M27.advance();
	results.push(instanceL4M27.read() + L4M27Kind.Done);
	const instanceL3M07 = new BaseL3M07();
	instanceL3M07.advance();
	results.push(instanceL3M07.read() + L3M07Kind.Idle);
	const instanceL0M05 = new BaseL0M05();
	instanceL0M05.advance();
	results.push(instanceL0M05.read() + L0M05Kind.Done);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
