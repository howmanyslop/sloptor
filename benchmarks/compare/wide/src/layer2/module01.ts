import { computeL1M09, L1M09Kind, BaseL1M09 } from "../layer1/module09";

export enum L2M01Kind {
	Failed,
	Idle,
	Queued,
	Active,
	Locked,
}

export const L2M01_FACTOR = 5;

export class BaseL2M01 {
	protected total = 37;
	public kind = L2M01Kind.Failed;

	public scale(value: number): number {
		this.total += value * L2M01_FACTOR;
		return this.total;
	}

	public advance(): L2M01Kind {
		const order = [L2M01Kind.Failed, L2M01Kind.Idle, L2M01Kind.Queued, L2M01Kind.Active, L2M01Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M01(input: number): number {
	let result = input * 5 + 37;
	result += computeL1M09(input % 11);
	return result % 27778;
}

export function describeL2M01(kind: L2M01Kind): string {
	switch (kind) {
		case L2M01Kind.Failed:
			return "failed";
		case L2M01Kind.Idle:
			return "idle";
		case L2M01Kind.Queued:
			return "queued";
		case L2M01Kind.Active:
			return "active";
		case L2M01Kind.Locked:
			return "locked";
	}
}

export function linkL2M01(): Array<number> {
	const results = new Array<number>();
	const instanceL1M09 = new BaseL1M09();
	instanceL1M09.advance();
	results.push(instanceL1M09.read() + L1M09Kind.Idle);
	return results;
}
