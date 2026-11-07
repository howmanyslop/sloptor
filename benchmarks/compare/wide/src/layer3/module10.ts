import { computeL2M33, L2M33Kind, BaseL2M33 } from "../layer2/module33";
import { computeL2M15, L2M15Kind, BaseL2M15 } from "../layer2/module15";
import { computeL2M36, L2M36Kind, BaseL2M36 } from "../layer2/module36";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L3M10Kind {
	Failed,
	Active,
	Queued,
	Idle,
	Done,
}

export const L3M10_FACTOR = 18;

export class BaseL3M10 {
	protected total = 71;
	public kind = L3M10Kind.Failed;

	public mix(value: number): number {
		this.total += value * L3M10_FACTOR;
		return this.total;
	}

	public advance(): L3M10Kind {
		const order = [L3M10Kind.Failed, L3M10Kind.Active, L3M10Kind.Queued, L3M10Kind.Idle, L3M10Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL3M10 extends BaseL2M33 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M10(input: number): number {
	let result = input * 18 + 71;
	result += computeL2M33(input % 6);
	result += computeL2M15(input % 3);
	result += computeL2M36(input % 4);
	result += computeL0M01(input % 11);
	return result % 43113;
}

export function describeL3M10(kind: L3M10Kind): string {
	switch (kind) {
		case L3M10Kind.Failed:
			return "failed";
		case L3M10Kind.Active:
			return "active";
		case L3M10Kind.Queued:
			return "queued";
		case L3M10Kind.Idle:
			return "idle";
		case L3M10Kind.Done:
			return "done";
	}
}

export function linkL3M10(): Array<number> {
	const results = new Array<number>();
	const instanceL2M33 = new BaseL2M33();
	instanceL2M33.advance();
	results.push(instanceL2M33.read() + L2M33Kind.Locked);
	const instanceL2M15 = new BaseL2M15();
	instanceL2M15.advance();
	results.push(instanceL2M15.read() + L2M15Kind.Active);
	const instanceL2M36 = new BaseL2M36();
	instanceL2M36.advance();
	results.push(instanceL2M36.read() + L2M36Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
