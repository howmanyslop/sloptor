import { computeL5M20, L5M20Kind, BaseL5M20 } from "../layer5/module20";
import { computeL1M38, L1M38Kind, BaseL1M38 } from "../layer1/module38";
import { computeL0M05, L0M05Kind, BaseL0M05 } from "../layer0/module05";

export enum L6M02Kind {
	Paused,
	Active,
	Failed,
}

export const L6M02_FACTOR = 14;

export class BaseL6M02 {
	protected total = 48;
	public kind = L6M02Kind.Paused;

	public warp(value: number): number {
		this.total += value * L6M02_FACTOR;
		return this.total;
	}

	public advance(): L6M02Kind {
		const order = [L6M02Kind.Paused, L6M02Kind.Active, L6M02Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL6M02 extends BaseL6M02 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 9) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M02(input: number): number {
	let result = input * 14 + 48;
	result += computeL5M20(input % 6);
	result += computeL1M38(input % 10);
	result += computeL0M05(input % 7);
	return result % 39768;
}

export function describeL6M02(kind: L6M02Kind): string {
	switch (kind) {
		case L6M02Kind.Paused:
			return "paused";
		case L6M02Kind.Active:
			return "active";
		case L6M02Kind.Failed:
			return "failed";
	}
}

export function linkL6M02(): Array<number> {
	const results = new Array<number>();
	const instanceL5M20 = new BaseL5M20();
	instanceL5M20.advance();
	results.push(instanceL5M20.read() + L5M20Kind.Locked);
	const instanceL1M38 = new BaseL1M38();
	instanceL1M38.advance();
	results.push(instanceL1M38.read() + L1M38Kind.Locked);
	const instanceL0M05 = new BaseL0M05();
	instanceL0M05.advance();
	results.push(instanceL0M05.read() + L0M05Kind.Done);
	return results;
}
