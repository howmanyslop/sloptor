import { computeL3M24, L3M24Kind, BaseL3M24 } from "../layer3/module24";
import { computeL1M10, L1M10Kind, BaseL1M10 } from "../layer1/module10";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L4M20Kind {
	Done,
	Paused,
	Active,
}

export const L4M20_FACTOR = 12;

export class BaseL4M20 {
	protected total = 90;
	public kind = L4M20Kind.Done;

	public shift(value: number): number {
		this.total += value * L4M20_FACTOR;
		return this.total;
	}

	public advance(): L4M20Kind {
		const order = [L4M20Kind.Done, L4M20Kind.Paused, L4M20Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL4M20 extends BaseL3M24 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M20(input: number): number {
	let result = input * 12 + 90;
	result += computeL3M24(input % 8);
	result += computeL1M10(input % 5);
	result += computeL0M01(input % 3);
	return result % 1796;
}

export function describeL4M20(kind: L4M20Kind): string {
	switch (kind) {
		case L4M20Kind.Done:
			return "done";
		case L4M20Kind.Paused:
			return "paused";
		case L4M20Kind.Active:
			return "active";
	}
}

export function linkL4M20(): Array<number> {
	const results = new Array<number>();
	const instanceL3M24 = new BaseL3M24();
	instanceL3M24.advance();
	results.push(instanceL3M24.read() + L3M24Kind.Idle);
	const instanceL1M10 = new BaseL1M10();
	instanceL1M10.advance();
	results.push(instanceL1M10.read() + L1M10Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
