import { computeL1M38, L1M38Kind, BaseL1M38 } from "../layer1/module38";
import { computeL1M07, L1M07Kind, BaseL1M07 } from "../layer1/module07";
import { computeL1M01, L1M01Kind, BaseL1M01 } from "../layer1/module01";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L2M28Kind {
	Failed,
	Idle,
	Locked,
}

export const L2M28_FACTOR = 19;

export class BaseL2M28 {
	protected total = 56;
	public kind = L2M28Kind.Failed;

	public tally(value: number): number {
		this.total += value * L2M28_FACTOR;
		return this.total;
	}

	public advance(): L2M28Kind {
		const order = [L2M28Kind.Failed, L2M28Kind.Idle, L2M28Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL2M28 extends BaseL2M28 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M28(input: number): number {
	let result = input * 19 + 56;
	result += computeL1M38(input % 9);
	result += computeL1M07(input % 8);
	result += computeL1M01(input % 4);
	result += computeL0M02(input % 5);
	return result % 11211;
}

export function describeL2M28(kind: L2M28Kind): string {
	switch (kind) {
		case L2M28Kind.Failed:
			return "failed";
		case L2M28Kind.Idle:
			return "idle";
		case L2M28Kind.Locked:
			return "locked";
	}
}

export function linkL2M28(): Array<number> {
	const results = new Array<number>();
	const instanceL1M38 = new BaseL1M38();
	instanceL1M38.advance();
	results.push(instanceL1M38.read() + L1M38Kind.Locked);
	const instanceL1M07 = new BaseL1M07();
	instanceL1M07.advance();
	results.push(instanceL1M07.read() + L1M07Kind.Idle);
	const instanceL1M01 = new BaseL1M01();
	instanceL1M01.advance();
	results.push(instanceL1M01.read() + L1M01Kind.Locked);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
