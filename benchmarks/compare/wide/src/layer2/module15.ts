import { computeL1M27, L1M27Kind, BaseL1M27 } from "../layer1/module27";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L2M15Kind {
	Active,
	Locked,
	Done,
	Idle,
}

export const L2M15_FACTOR = 18;

export class BaseL2M15 {
	protected total = 20;
	public kind = L2M15Kind.Active;

	public blend(value: number): number {
		this.total += value * L2M15_FACTOR;
		return this.total;
	}

	public advance(): L2M15Kind {
		const order = [L2M15Kind.Active, L2M15Kind.Locked, L2M15Kind.Done, L2M15Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M15(input: number): number {
	let result = input * 18 + 20;
	result += computeL1M27(input % 11);
	result += computeL0M00(input % 5);
	return result % 46100;
}

export function describeL2M15(kind: L2M15Kind): string {
	switch (kind) {
		case L2M15Kind.Active:
			return "active";
		case L2M15Kind.Locked:
			return "locked";
		case L2M15Kind.Done:
			return "done";
		case L2M15Kind.Idle:
			return "idle";
	}
}

export function linkL2M15(): Array<number> {
	const results = new Array<number>();
	const instanceL1M27 = new BaseL1M27();
	instanceL1M27.advance();
	results.push(instanceL1M27.read() + L1M27Kind.Paused);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
