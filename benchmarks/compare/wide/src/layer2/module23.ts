import { computeL1M03, L1M03Kind, BaseL1M03 } from "../layer1/module03";
import { computeL0M06, L0M06Kind, BaseL0M06 } from "../layer0/module06";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L2M23Kind {
	Queued,
	Idle,
	Locked,
	Active,
	Paused,
}

export const L2M23_FACTOR = 14;

export class BaseL2M23 {
	protected total = 49;
	public kind = L2M23Kind.Queued;

	public blend(value: number): number {
		this.total += value * L2M23_FACTOR;
		return this.total;
	}

	public advance(): L2M23Kind {
		const order = [L2M23Kind.Queued, L2M23Kind.Idle, L2M23Kind.Locked, L2M23Kind.Active, L2M23Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL2M23 extends BaseL1M03 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M23(input: number): number {
	let result = input * 14 + 49;
	result += computeL1M03(input % 7);
	result += computeL0M06(input % 4);
	result += computeL0M01(input % 9);
	result += computeL0M00(input % 9);
	return result % 37609;
}

export function describeL2M23(kind: L2M23Kind): string {
	switch (kind) {
		case L2M23Kind.Queued:
			return "queued";
		case L2M23Kind.Idle:
			return "idle";
		case L2M23Kind.Locked:
			return "locked";
		case L2M23Kind.Active:
			return "active";
		case L2M23Kind.Paused:
			return "paused";
	}
}

export function linkL2M23(): Array<number> {
	const results = new Array<number>();
	const instanceL1M03 = new BaseL1M03();
	instanceL1M03.advance();
	results.push(instanceL1M03.read() + L1M03Kind.Locked);
	const instanceL0M06 = new BaseL0M06();
	instanceL0M06.advance();
	results.push(instanceL0M06.read() + L0M06Kind.Failed);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
