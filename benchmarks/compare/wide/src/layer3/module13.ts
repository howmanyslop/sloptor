import { computeL2M14, L2M14Kind, BaseL2M14 } from "../layer2/module14";
import { computeL0M15, L0M15Kind, BaseL0M15 } from "../layer0/module15";
import { computeL2M02, L2M02Kind, BaseL2M02 } from "../layer2/module02";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L3M13Kind {
	Queued,
	Failed,
	Active,
	Idle,
}

export const L3M13_FACTOR = 13;

export class BaseL3M13 {
	protected total = 73;
	public kind = L3M13Kind.Queued;

	public mix(value: number): number {
		this.total += value * L3M13_FACTOR;
		return this.total;
	}

	public advance(): L3M13Kind {
		const order = [L3M13Kind.Queued, L3M13Kind.Failed, L3M13Kind.Active, L3M13Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M13(input: number): number {
	let result = input * 13 + 73;
	result += computeL2M14(input % 4);
	result += computeL0M15(input % 5);
	result += computeL2M02(input % 7);
	result += computeL0M02(input % 8);
	return result % 79229;
}

export function describeL3M13(kind: L3M13Kind): string {
	switch (kind) {
		case L3M13Kind.Queued:
			return "queued";
		case L3M13Kind.Failed:
			return "failed";
		case L3M13Kind.Active:
			return "active";
		case L3M13Kind.Idle:
			return "idle";
	}
}

export function linkL3M13(): Array<number> {
	const results = new Array<number>();
	const instanceL2M14 = new BaseL2M14();
	instanceL2M14.advance();
	results.push(instanceL2M14.read() + L2M14Kind.Queued);
	const instanceL0M15 = new BaseL0M15();
	instanceL0M15.advance();
	results.push(instanceL0M15.read() + L0M15Kind.Paused);
	const instanceL2M02 = new BaseL2M02();
	instanceL2M02.advance();
	results.push(instanceL2M02.read() + L2M02Kind.Active);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
