import { computeL0M15, L0M15Kind, BaseL0M15 } from "../layer0/module15";
import { computeL0M05, L0M05Kind, BaseL0M05 } from "../layer0/module05";

export enum L1M02Kind {
	Locked,
	Idle,
	Active,
	Done,
}

export const L1M02_FACTOR = 3;

export class BaseL1M02 {
	protected total = 6;
	public kind = L1M02Kind.Locked;

	public warp(value: number): number {
		this.total += value * L1M02_FACTOR;
		return this.total;
	}

	public advance(): L1M02Kind {
		const order = [L1M02Kind.Locked, L1M02Kind.Idle, L1M02Kind.Active, L1M02Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL1M02 extends BaseL0M15 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M02(input: number): number {
	let result = input * 3 + 6;
	result += computeL0M15(input % 10);
	result += computeL0M05(input % 8);
	return result % 39264;
}

export const L1M02_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 6,
	chain: (value) => computeL1M02(value),
};

export function linkL1M02(): Array<number> {
	const results = new Array<number>();
	const instanceL0M15 = new BaseL0M15();
	instanceL0M15.advance();
	results.push(instanceL0M15.read() + L0M15Kind.Paused);
	const instanceL0M05 = new BaseL0M05();
	instanceL0M05.advance();
	results.push(instanceL0M05.read() + L0M05Kind.Done);
	return results;
}
