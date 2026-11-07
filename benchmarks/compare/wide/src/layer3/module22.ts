import { computeL2M37, L2M37Kind, BaseL2M37 } from "../layer2/module37";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";
import { computeL0M27, L0M27Kind, BaseL0M27 } from "../layer0/module27";

export enum L3M22Kind {
	Paused,
	Idle,
	Done,
	Failed,
	Active,
}

export const L3M22_FACTOR = 16;

export class BaseL3M22 {
	protected total = 43;
	public kind = L3M22Kind.Paused;

	public blend(value: number): number {
		this.total += value * L3M22_FACTOR;
		return this.total;
	}

	public advance(): L3M22Kind {
		const order = [L3M22Kind.Paused, L3M22Kind.Idle, L3M22Kind.Done, L3M22Kind.Failed, L3M22Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL3M22 extends BaseL2M37 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 11) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M22(input: number): number {
	let result = input * 16 + 43;
	result += computeL2M37(input % 6);
	result += computeL0M02(input % 7);
	result += computeL0M27(input % 5);
	return result % 53242;
}

export function describeL3M22(kind: L3M22Kind): string {
	switch (kind) {
		case L3M22Kind.Paused:
			return "paused";
		case L3M22Kind.Idle:
			return "idle";
		case L3M22Kind.Done:
			return "done";
		case L3M22Kind.Failed:
			return "failed";
		case L3M22Kind.Active:
			return "active";
	}
}

export function linkL3M22(): Array<number> {
	const results = new Array<number>();
	const instanceL2M37 = new BaseL2M37();
	instanceL2M37.advance();
	results.push(instanceL2M37.read() + L2M37Kind.Failed);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	const instanceL0M27 = new BaseL0M27();
	instanceL0M27.advance();
	results.push(instanceL0M27.read() + L0M27Kind.Done);
	return results;
}
