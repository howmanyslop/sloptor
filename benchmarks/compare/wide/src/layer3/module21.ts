import { computeL2M30, L2M30Kind, BaseL2M30 } from "../layer2/module30";
import { computeL1M01, L1M01Kind, BaseL1M01 } from "../layer1/module01";
import { computeL0M17, L0M17Kind, BaseL0M17 } from "../layer0/module17";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L3M21Kind {
	Done,
	Failed,
	Paused,
}

export const L3M21_FACTOR = 10;

export class BaseL3M21 {
	protected total = 69;
	public kind = L3M21Kind.Done;

	public warp(value: number): number {
		this.total += value * L3M21_FACTOR;
		return this.total;
	}

	public advance(): L3M21Kind {
		const order = [L3M21Kind.Done, L3M21Kind.Failed, L3M21Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL3M21 extends BaseL3M21 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M21(input: number): number {
	let result = input * 10 + 69;
	result += computeL2M30(input % 7);
	result += computeL1M01(input % 3);
	result += computeL0M17(input % 7);
	result += computeL0M01(input % 11);
	return result % 75360;
}

export const L3M21_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 69,
	chain: (value) => computeL3M21(value),
};

export function linkL3M21(): Array<number> {
	const results = new Array<number>();
	const instanceL2M30 = new BaseL2M30();
	instanceL2M30.advance();
	results.push(instanceL2M30.read() + L2M30Kind.Done);
	const instanceL1M01 = new BaseL1M01();
	instanceL1M01.advance();
	results.push(instanceL1M01.read() + L1M01Kind.Locked);
	const instanceL0M17 = new BaseL0M17();
	instanceL0M17.advance();
	results.push(instanceL0M17.read() + L0M17Kind.Paused);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
