import { BaseL0M02, computeL0M02, L0M02Kind } from "../layer0/module02";
import { BaseL0M14, computeL0M14, L0M14Kind } from "../layer0/module14";
import { BaseL4M33, computeL4M33, L4M33Kind } from "../layer4/module33";

export enum L5M04Kind {
	Active,
	Done,
	Queued,
	Failed,
	Paused,
}

export const L5M04_FACTOR = 18;

export class BaseL5M04 {
	protected total = 97;

	public kind = L5M04Kind.Active;

	public advance(): L5M04Kind {
		const order = [L5M04Kind.Active, L5M04Kind.Done, L5M04Kind.Queued, L5M04Kind.Failed, L5M04Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}

	public tally(value: number): number {
		this.total += value * L5M04_FACTOR;
		return this.total;
	}
}

export class VesselL5M04 extends BaseL5M04 {
	private readonly history = new Array<number>();

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}
}

export function computeL5M04(input: number): number {
	let result = input * 18 + 97;
	result += computeL4M33(input % 4);
	result += computeL0M14(input % 4);
	result += computeL0M02(input % 3);
	return result % 35053;
}

export function describeL5M04(kind: L5M04Kind): string {
	switch (kind) {
		case L5M04Kind.Active:
			return "active";
		case L5M04Kind.Done:
			return "done";
		case L5M04Kind.Failed:
			return "failed";
		case L5M04Kind.Paused:
			return "paused";
		case L5M04Kind.Queued:
			return "queued";
	}
}

export function linkL5M04(): Array<number> {
	const results = new Array<number>();
	const instanceL4M33 = new BaseL4M33();
	instanceL4M33.advance();
	results.push(instanceL4M33.read() + L4M33Kind.Queued);
	const instanceL0M14 = new BaseL0M14();
	instanceL0M14.advance();
	results.push(instanceL0M14.read() + L0M14Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
