import { computeL2M38, L2M38Kind, BaseL2M38 } from "../layer2/module38";
import { computeL1M28, L1M28Kind, BaseL1M28 } from "../layer1/module28";
import { computeL2M12, L2M12Kind, BaseL2M12 } from "../layer2/module12";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L3M05Kind {
	Failed,
	Active,
	Done,
	Locked,
	Paused,
}

export const L3M05_FACTOR = 5;

export class BaseL3M05 {
	protected total = 89;
	public kind = L3M05Kind.Failed;

	public fold(value: number): number {
		this.total += value * L3M05_FACTOR;
		return this.total;
	}

	public advance(): L3M05Kind {
		const order = [L3M05Kind.Failed, L3M05Kind.Active, L3M05Kind.Done, L3M05Kind.Locked, L3M05Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL3M05 extends BaseL2M38 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M05(input: number): number {
	let result = input * 5 + 89;
	result += computeL2M38(input % 8);
	result += computeL1M28(input % 9);
	result += computeL2M12(input % 7);
	result += computeL0M02(input % 11);
	return result % 14354;
}

export function describeL3M05(kind: L3M05Kind): string {
	switch (kind) {
		case L3M05Kind.Failed:
			return "failed";
		case L3M05Kind.Active:
			return "active";
		case L3M05Kind.Done:
			return "done";
		case L3M05Kind.Locked:
			return "locked";
		case L3M05Kind.Paused:
			return "paused";
	}
}

export function linkL3M05(): Array<number> {
	const results = new Array<number>();
	const instanceL2M38 = new BaseL2M38();
	instanceL2M38.advance();
	results.push(instanceL2M38.read() + L2M38Kind.Done);
	const instanceL1M28 = new BaseL1M28();
	instanceL1M28.advance();
	results.push(instanceL1M28.read() + L1M28Kind.Done);
	const instanceL2M12 = new BaseL2M12();
	instanceL2M12.advance();
	results.push(instanceL2M12.read() + L2M12Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
