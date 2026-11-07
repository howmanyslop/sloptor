import { computeL1M05, L1M05Kind, BaseL1M05 } from "../layer1/module05";
import { computeL1M09, L1M09Kind, BaseL1M09 } from "../layer1/module09";
import { computeL0M32, L0M32Kind, BaseL0M32 } from "../layer0/module32";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L2M29Kind {
	Locked,
	Idle,
	Paused,
}

export const L2M29_FACTOR = 2;

export class BaseL2M29 {
	protected total = 20;
	public kind = L2M29Kind.Locked;

	public shift(value: number): number {
		this.total += value * L2M29_FACTOR;
		return this.total;
	}

	public advance(): L2M29Kind {
		const order = [L2M29Kind.Locked, L2M29Kind.Idle, L2M29Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M29(input: number): number {
	let result = input * 2 + 20;
	result += computeL1M05(input % 9);
	result += computeL1M09(input % 9);
	result += computeL0M32(input % 6);
	result += computeL0M02(input % 9);
	return result % 28325;
}

export function describeL2M29(kind: L2M29Kind): string {
	switch (kind) {
		case L2M29Kind.Locked:
			return "locked";
		case L2M29Kind.Idle:
			return "idle";
		case L2M29Kind.Paused:
			return "paused";
	}
}

export function linkL2M29(): Array<number> {
	const results = new Array<number>();
	const instanceL1M05 = new BaseL1M05();
	instanceL1M05.advance();
	results.push(instanceL1M05.read() + L1M05Kind.Idle);
	const instanceL1M09 = new BaseL1M09();
	instanceL1M09.advance();
	results.push(instanceL1M09.read() + L1M09Kind.Idle);
	const instanceL0M32 = new BaseL0M32();
	instanceL0M32.advance();
	results.push(instanceL0M32.read() + L0M32Kind.Paused);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
