import { computeL6M15, L6M15Kind, BaseL6M15 } from "../layer6/module15";
import { computeL6M20, L6M20Kind, BaseL6M20 } from "../layer6/module20";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L7M24Kind {
	Idle,
	Queued,
	Locked,
}

export const L7M24_FACTOR = 2;

export class BaseL7M24 {
	protected total = 89;
	public kind = L7M24Kind.Idle;

	public scale(value: number): number {
		this.total += value * L7M24_FACTOR;
		return this.total;
	}

	public advance(): L7M24Kind {
		const order = [L7M24Kind.Idle, L7M24Kind.Queued, L7M24Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL7M24 extends BaseL6M15 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M24(input: number): number {
	let result = input * 2 + 89;
	result += computeL6M15(input % 11);
	result += computeL6M20(input % 11);
	result += computeL0M01(input % 4);
	return result % 54968;
}

export function describeL7M24(kind: L7M24Kind): string {
	switch (kind) {
		case L7M24Kind.Idle:
			return "idle";
		case L7M24Kind.Queued:
			return "queued";
		case L7M24Kind.Locked:
			return "locked";
	}
}

export function linkL7M24(): Array<number> {
	const results = new Array<number>();
	const instanceL6M15 = new BaseL6M15();
	instanceL6M15.advance();
	results.push(instanceL6M15.read() + L6M15Kind.Idle);
	const instanceL6M20 = new BaseL6M20();
	instanceL6M20.advance();
	results.push(instanceL6M20.read() + L6M20Kind.Failed);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
