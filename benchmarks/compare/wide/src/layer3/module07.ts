import { computeL2M12, L2M12Kind, BaseL2M12 } from "../layer2/module12";
import { computeL1M05, L1M05Kind, BaseL1M05 } from "../layer1/module05";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L3M07Kind {
	Idle,
	Queued,
	Active,
	Locked,
}

export const L3M07_FACTOR = 18;

export class BaseL3M07 {
	protected total = 6;
	public kind = L3M07Kind.Idle;

	public blend(value: number): number {
		this.total += value * L3M07_FACTOR;
		return this.total;
	}

	public advance(): L3M07Kind {
		const order = [L3M07Kind.Idle, L3M07Kind.Queued, L3M07Kind.Active, L3M07Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL3M07 extends BaseL3M07 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M07(input: number): number {
	let result = input * 18 + 6;
	result += computeL2M12(input % 3);
	result += computeL1M05(input % 7);
	result += computeL0M00(input % 6);
	return result % 77937;
}

export function describeL3M07(kind: L3M07Kind): string {
	switch (kind) {
		case L3M07Kind.Idle:
			return "idle";
		case L3M07Kind.Queued:
			return "queued";
		case L3M07Kind.Active:
			return "active";
		case L3M07Kind.Locked:
			return "locked";
	}
}

export function linkL3M07(): Array<number> {
	const results = new Array<number>();
	const instanceL2M12 = new BaseL2M12();
	instanceL2M12.advance();
	results.push(instanceL2M12.read() + L2M12Kind.Idle);
	const instanceL1M05 = new BaseL1M05();
	instanceL1M05.advance();
	results.push(instanceL1M05.read() + L1M05Kind.Idle);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
