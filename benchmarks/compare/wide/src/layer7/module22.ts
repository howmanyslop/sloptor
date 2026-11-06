import { computeL6M01, L6M01Kind, BaseL6M01 } from "../layer6/module01";
import { computeL6M26, L6M26Kind, BaseL6M26 } from "../layer6/module26";
import { computeL5M01, L5M01Kind, BaseL5M01 } from "../layer5/module01";

export enum L7M22Kind {
	Paused,
	Active,
	Queued,
	Failed,
	Locked,
}

export const L7M22_FACTOR = 12;

export class BaseL7M22 {
	protected total = 46;
	public kind = L7M22Kind.Paused;

	public tally(value: number): number {
		this.total += value * L7M22_FACTOR;
		return this.total;
	}

	public advance(): L7M22Kind {
		const order = [L7M22Kind.Paused, L7M22Kind.Active, L7M22Kind.Queued, L7M22Kind.Failed, L7M22Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL7M22 extends BaseL7M22 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 14) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M22(input: number): number {
	let result = input * 12 + 46;
	result += computeL6M01(input % 8);
	result += computeL6M26(input % 9);
	result += computeL5M01(input % 5);
	return result % 32120;
}

export function describeL7M22(kind: L7M22Kind): string {
	switch (kind) {
		case L7M22Kind.Paused:
			return "paused";
		case L7M22Kind.Active:
			return "active";
		case L7M22Kind.Queued:
			return "queued";
		case L7M22Kind.Failed:
			return "failed";
		case L7M22Kind.Locked:
			return "locked";
	}
}

export function linkL7M22(): Array<number> {
	const results = new Array<number>();
	const instanceL6M01 = new BaseL6M01();
	instanceL6M01.advance();
	results.push(instanceL6M01.read() + L6M01Kind.Queued);
	const instanceL6M26 = new BaseL6M26();
	instanceL6M26.advance();
	results.push(instanceL6M26.read() + L6M26Kind.Locked);
	const instanceL5M01 = new BaseL5M01();
	instanceL5M01.advance();
	results.push(instanceL5M01.read() + L5M01Kind.Idle);
	return results;
}
