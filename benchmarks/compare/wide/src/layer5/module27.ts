import { computeL4M34, L4M34Kind, BaseL4M34 } from "../layer4/module34";
import { computeL1M14, L1M14Kind, BaseL1M14 } from "../layer1/module14";
import { computeL4M24, L4M24Kind, BaseL4M24 } from "../layer4/module24";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L5M27Kind {
	Paused,
	Done,
	Failed,
}

export const L5M27_FACTOR = 4;

export class BaseL5M27 {
	protected total = 33;
	public kind = L5M27Kind.Paused;

	public scale(value: number): number {
		this.total += value * L5M27_FACTOR;
		return this.total;
	}

	public advance(): L5M27Kind {
		const order = [L5M27Kind.Paused, L5M27Kind.Done, L5M27Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M27(input: number): number {
	let result = input * 4 + 33;
	result += computeL4M34(input % 4);
	result += computeL1M14(input % 7);
	result += computeL4M24(input % 9);
	result += computeL0M00(input % 10);
	return result % 30881;
}

export function describeL5M27(kind: L5M27Kind): string {
	switch (kind) {
		case L5M27Kind.Paused:
			return "paused";
		case L5M27Kind.Done:
			return "done";
		case L5M27Kind.Failed:
			return "failed";
	}
}

export function linkL5M27(): Array<number> {
	const results = new Array<number>();
	const instanceL4M34 = new BaseL4M34();
	instanceL4M34.advance();
	results.push(instanceL4M34.read() + L4M34Kind.Paused);
	const instanceL1M14 = new BaseL1M14();
	instanceL1M14.advance();
	results.push(instanceL1M14.read() + L1M14Kind.Queued);
	const instanceL4M24 = new BaseL4M24();
	instanceL4M24.advance();
	results.push(instanceL4M24.read() + L4M24Kind.Queued);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
