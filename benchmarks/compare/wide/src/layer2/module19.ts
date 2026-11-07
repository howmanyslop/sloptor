import { computeL1M15, L1M15Kind, BaseL1M15 } from "../layer1/module15";
import { computeL0M13, L0M13Kind, BaseL0M13 } from "../layer0/module13";

export enum L2M19Kind {
	Active,
	Done,
	Locked,
	Paused,
}

export const L2M19_FACTOR = 14;

export class BaseL2M19 {
	protected total = 75;
	public kind = L2M19Kind.Active;

	public scale(value: number): number {
		this.total += value * L2M19_FACTOR;
		return this.total;
	}

	public advance(): L2M19Kind {
		const order = [L2M19Kind.Active, L2M19Kind.Done, L2M19Kind.Locked, L2M19Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M19(input: number): number {
	let result = input * 14 + 75;
	result += computeL1M15(input % 3);
	result += computeL0M13(input % 9);
	return result % 61329;
}

export function describeL2M19(kind: L2M19Kind): string {
	switch (kind) {
		case L2M19Kind.Active:
			return "active";
		case L2M19Kind.Done:
			return "done";
		case L2M19Kind.Locked:
			return "locked";
		case L2M19Kind.Paused:
			return "paused";
	}
}

export function linkL2M19(): Array<number> {
	const results = new Array<number>();
	const instanceL1M15 = new BaseL1M15();
	instanceL1M15.advance();
	results.push(instanceL1M15.read() + L1M15Kind.Done);
	const instanceL0M13 = new BaseL0M13();
	instanceL0M13.advance();
	results.push(instanceL0M13.read() + L0M13Kind.Idle);
	return results;
}
