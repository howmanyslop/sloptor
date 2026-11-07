import { computeL2M13, L2M13Kind, BaseL2M13 } from "../layer2/module13";
import { computeL1M02, L1M02Kind, BaseL1M02 } from "../layer1/module02";
import { computeL0M04, L0M04Kind, BaseL0M04 } from "../layer0/module04";

export enum L3M30Kind {
	Done,
	Idle,
	Active,
}

export const L3M30_FACTOR = 7;

export class BaseL3M30 {
	protected total = 28;
	public kind = L3M30Kind.Done;

	public shift(value: number): number {
		this.total += value * L3M30_FACTOR;
		return this.total;
	}

	public advance(): L3M30Kind {
		const order = [L3M30Kind.Done, L3M30Kind.Idle, L3M30Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M30(input: number): number {
	let result = input * 7 + 28;
	result += computeL2M13(input % 5);
	result += computeL1M02(input % 6);
	result += computeL0M04(input % 5);
	return result % 92148;
}

export function describeL3M30(kind: L3M30Kind): string {
	switch (kind) {
		case L3M30Kind.Done:
			return "done";
		case L3M30Kind.Idle:
			return "idle";
		case L3M30Kind.Active:
			return "active";
	}
}

export function linkL3M30(): Array<number> {
	const results = new Array<number>();
	const instanceL2M13 = new BaseL2M13();
	instanceL2M13.advance();
	results.push(instanceL2M13.read() + L2M13Kind.Failed);
	const instanceL1M02 = new BaseL1M02();
	instanceL1M02.advance();
	results.push(instanceL1M02.read() + L1M02Kind.Locked);
	const instanceL0M04 = new BaseL0M04();
	instanceL0M04.advance();
	results.push(instanceL0M04.read() + L0M04Kind.Paused);
	return results;
}
