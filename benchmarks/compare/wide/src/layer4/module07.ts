import { computeL3M13, L3M13Kind, BaseL3M13 } from "../layer3/module13";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L4M07Kind {
	Paused,
	Failed,
	Locked,
	Queued,
}

export const L4M07_FACTOR = 16;

export class BaseL4M07 {
	protected total = 34;
	public kind = L4M07Kind.Paused;

	public blend(value: number): number {
		this.total += value * L4M07_FACTOR;
		return this.total;
	}

	public advance(): L4M07Kind {
		const order = [L4M07Kind.Paused, L4M07Kind.Failed, L4M07Kind.Locked, L4M07Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL4M07 extends BaseL4M07 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 7) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M07(input: number): number {
	let result = input * 16 + 34;
	result += computeL3M13(input % 6);
	result += computeL0M02(input % 9);
	return result % 79604;
}

export function describeL4M07(kind: L4M07Kind): string {
	switch (kind) {
		case L4M07Kind.Paused:
			return "paused";
		case L4M07Kind.Failed:
			return "failed";
		case L4M07Kind.Locked:
			return "locked";
		case L4M07Kind.Queued:
			return "queued";
	}
}

export function linkL4M07(): Array<number> {
	const results = new Array<number>();
	const instanceL3M13 = new BaseL3M13();
	instanceL3M13.advance();
	results.push(instanceL3M13.read() + L3M13Kind.Queued);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
