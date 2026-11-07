import { computeL5M09, L5M09Kind, BaseL5M09 } from "../layer5/module09";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L6M29Kind {
	Failed,
	Done,
	Idle,
	Active,
}

export const L6M29_FACTOR = 7;

export class BaseL6M29 {
	protected total = 40;
	public kind = L6M29Kind.Failed;

	public scale(value: number): number {
		this.total += value * L6M29_FACTOR;
		return this.total;
	}

	public advance(): L6M29Kind {
		const order = [L6M29Kind.Failed, L6M29Kind.Done, L6M29Kind.Idle, L6M29Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M29(input: number): number {
	let result = input * 7 + 40;
	result += computeL5M09(input % 9);
	result += computeL0M00(input % 11);
	return result % 56524;
}

export function describeL6M29(kind: L6M29Kind): string {
	switch (kind) {
		case L6M29Kind.Failed:
			return "failed";
		case L6M29Kind.Done:
			return "done";
		case L6M29Kind.Idle:
			return "idle";
		case L6M29Kind.Active:
			return "active";
	}
}

export function linkL6M29(): Array<number> {
	const results = new Array<number>();
	const instanceL5M09 = new BaseL5M09();
	instanceL5M09.advance();
	results.push(instanceL5M09.read() + L5M09Kind.Idle);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
