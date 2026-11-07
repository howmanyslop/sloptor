import { computeL0M28, L0M28Kind, BaseL0M28 } from "../layer0/module28";
import { computeL0M39, L0M39Kind, BaseL0M39 } from "../layer0/module39";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L1M21Kind {
	Failed,
	Active,
	Queued,
}

export const L1M21_FACTOR = 3;

export class BaseL1M21 {
	protected total = 46;
	public kind = L1M21Kind.Failed;

	public weigh(value: number): number {
		this.total += value * L1M21_FACTOR;
		return this.total;
	}

	public advance(): L1M21Kind {
		const order = [L1M21Kind.Failed, L1M21Kind.Active, L1M21Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M21(input: number): number {
	let result = input * 3 + 46;
	result += computeL0M28(input % 9);
	result += computeL0M39(input % 7);
	result += computeL0M02(input % 7);
	return result % 46588;
}

export function describeL1M21(kind: L1M21Kind): string {
	switch (kind) {
		case L1M21Kind.Failed:
			return "failed";
		case L1M21Kind.Active:
			return "active";
		case L1M21Kind.Queued:
			return "queued";
	}
}

export function linkL1M21(): Array<number> {
	const results = new Array<number>();
	const instanceL0M28 = new BaseL0M28();
	instanceL0M28.advance();
	results.push(instanceL0M28.read() + L0M28Kind.Active);
	const instanceL0M39 = new BaseL0M39();
	instanceL0M39.advance();
	results.push(instanceL0M39.read() + L0M39Kind.Active);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
