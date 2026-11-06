import { computeL5M24, L5M24Kind, BaseL5M24 } from "../layer5/module24";
import { computeL1M14, L1M14Kind, BaseL1M14 } from "../layer1/module14";

export enum L6M14Kind {
	Idle,
	Active,
	Paused,
	Done,
	Failed,
}

export const L6M14_FACTOR = 8;

export class BaseL6M14 {
	protected total = 95;
	public kind = L6M14Kind.Idle;

	public scale(value: number): number {
		this.total += value * L6M14_FACTOR;
		return this.total;
	}

	public advance(): L6M14Kind {
		const order = [L6M14Kind.Idle, L6M14Kind.Active, L6M14Kind.Paused, L6M14Kind.Done, L6M14Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M14(input: number): number {
	let result = input * 8 + 95;
	result += computeL5M24(input % 6);
	result += computeL1M14(input % 8);
	return result % 26672;
}

export function describeL6M14(kind: L6M14Kind): string {
	switch (kind) {
		case L6M14Kind.Idle:
			return "idle";
		case L6M14Kind.Active:
			return "active";
		case L6M14Kind.Paused:
			return "paused";
		case L6M14Kind.Done:
			return "done";
		case L6M14Kind.Failed:
			return "failed";
	}
}

export function linkL6M14(): Array<number> {
	const results = new Array<number>();
	const instanceL5M24 = new BaseL5M24();
	instanceL5M24.advance();
	results.push(instanceL5M24.read() + L5M24Kind.Failed);
	const instanceL1M14 = new BaseL1M14();
	instanceL1M14.advance();
	results.push(instanceL1M14.read() + L1M14Kind.Queued);
	return results;
}
