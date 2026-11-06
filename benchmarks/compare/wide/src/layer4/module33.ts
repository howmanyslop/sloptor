import { computeL3M35, L3M35Kind, BaseL3M35 } from "../layer3/module35";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L4M33Kind {
	Queued,
	Failed,
	Locked,
	Done,
	Paused,
}

export const L4M33_FACTOR = 5;

export class BaseL4M33 {
	protected total = 15;
	public kind = L4M33Kind.Queued;

	public warp(value: number): number {
		this.total += value * L4M33_FACTOR;
		return this.total;
	}

	public advance(): L4M33Kind {
		const order = [L4M33Kind.Queued, L4M33Kind.Failed, L4M33Kind.Locked, L4M33Kind.Done, L4M33Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M33(input: number): number {
	let result = input * 5 + 15;
	result += computeL3M35(input % 8);
	result += computeL0M03(input % 11);
	return result % 11867;
}

export function describeL4M33(kind: L4M33Kind): string {
	switch (kind) {
		case L4M33Kind.Queued:
			return "queued";
		case L4M33Kind.Failed:
			return "failed";
		case L4M33Kind.Locked:
			return "locked";
		case L4M33Kind.Done:
			return "done";
		case L4M33Kind.Paused:
			return "paused";
	}
}

export function linkL4M33(): Array<number> {
	const results = new Array<number>();
	const instanceL3M35 = new BaseL3M35();
	instanceL3M35.advance();
	results.push(instanceL3M35.read() + L3M35Kind.Idle);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
