import { computeL1M01, L1M01Kind, BaseL1M01 } from "../layer1/module01";
import { computeL0M13, L0M13Kind, BaseL0M13 } from "../layer0/module13";
import { computeL0M30, L0M30Kind, BaseL0M30 } from "../layer0/module30";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L2M17Kind {
	Idle,
	Active,
	Queued,
	Paused,
}

export const L2M17_FACTOR = 16;

export class BaseL2M17 {
	protected total = 16;
	public kind = L2M17Kind.Idle;

	public warp(value: number): number {
		this.total += value * L2M17_FACTOR;
		return this.total;
	}

	public advance(): L2M17Kind {
		const order = [L2M17Kind.Idle, L2M17Kind.Active, L2M17Kind.Queued, L2M17Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M17(input: number): number {
	let result = input * 16 + 16;
	result += computeL1M01(input % 8);
	result += computeL0M13(input % 5);
	result += computeL0M30(input % 3);
	result += computeL0M03(input % 10);
	return result % 82626;
}

export function describeL2M17(kind: L2M17Kind): string {
	switch (kind) {
		case L2M17Kind.Idle:
			return "idle";
		case L2M17Kind.Active:
			return "active";
		case L2M17Kind.Queued:
			return "queued";
		case L2M17Kind.Paused:
			return "paused";
	}
}

export function linkL2M17(): Array<number> {
	const results = new Array<number>();
	const instanceL1M01 = new BaseL1M01();
	instanceL1M01.advance();
	results.push(instanceL1M01.read() + L1M01Kind.Locked);
	const instanceL0M13 = new BaseL0M13();
	instanceL0M13.advance();
	results.push(instanceL0M13.read() + L0M13Kind.Idle);
	const instanceL0M30 = new BaseL0M30();
	instanceL0M30.advance();
	results.push(instanceL0M30.read() + L0M30Kind.Done);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
