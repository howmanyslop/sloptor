import { computeL0M37, L0M37Kind, BaseL0M37 } from "../layer0/module37";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L1M28Kind {
	Done,
	Locked,
	Paused,
	Queued,
	Active,
}

export const L1M28_FACTOR = 17;

export class BaseL1M28 {
	protected total = 23;
	public kind = L1M28Kind.Done;

	public scale(value: number): number {
		this.total += value * L1M28_FACTOR;
		return this.total;
	}

	public advance(): L1M28Kind {
		const order = [L1M28Kind.Done, L1M28Kind.Locked, L1M28Kind.Paused, L1M28Kind.Queued, L1M28Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL1M28(input: number): number {
	let result = input * 17 + 23;
	result += computeL0M37(input % 7);
	result += computeL0M00(input % 3);
	return result % 69147;
}

export function describeL1M28(kind: L1M28Kind): string {
	switch (kind) {
		case L1M28Kind.Done:
			return "done";
		case L1M28Kind.Locked:
			return "locked";
		case L1M28Kind.Paused:
			return "paused";
		case L1M28Kind.Queued:
			return "queued";
		case L1M28Kind.Active:
			return "active";
	}
}

export function linkL1M28(): Array<number> {
	const results = new Array<number>();
	const instanceL0M37 = new BaseL0M37();
	instanceL0M37.advance();
	results.push(instanceL0M37.read() + L0M37Kind.Done);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
