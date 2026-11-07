import { computeL1M28, L1M28Kind, BaseL1M28 } from "../layer1/module28";
import { computeL1M25, L1M25Kind, BaseL1M25 } from "../layer1/module25";
import { computeL0M05, L0M05Kind, BaseL0M05 } from "../layer0/module05";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L2M38Kind {
	Done,
	Queued,
	Paused,
}

export const L2M38_FACTOR = 15;

export class BaseL2M38 {
	protected total = 23;
	public kind = L2M38Kind.Done;

	public scale(value: number): number {
		this.total += value * L2M38_FACTOR;
		return this.total;
	}

	public advance(): L2M38Kind {
		const order = [L2M38Kind.Done, L2M38Kind.Queued, L2M38Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M38(input: number): number {
	let result = input * 15 + 23;
	result += computeL1M28(input % 10);
	result += computeL1M25(input % 4);
	result += computeL0M05(input % 10);
	result += computeL0M00(input % 6);
	return result % 59980;
}

export function describeL2M38(kind: L2M38Kind): string {
	switch (kind) {
		case L2M38Kind.Done:
			return "done";
		case L2M38Kind.Queued:
			return "queued";
		case L2M38Kind.Paused:
			return "paused";
	}
}

export function linkL2M38(): Array<number> {
	const results = new Array<number>();
	const instanceL1M28 = new BaseL1M28();
	instanceL1M28.advance();
	results.push(instanceL1M28.read() + L1M28Kind.Done);
	const instanceL1M25 = new BaseL1M25();
	instanceL1M25.advance();
	results.push(instanceL1M25.read() + L1M25Kind.Queued);
	const instanceL0M05 = new BaseL0M05();
	instanceL0M05.advance();
	results.push(instanceL0M05.read() + L0M05Kind.Done);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
