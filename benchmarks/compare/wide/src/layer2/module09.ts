import { computeL1M25, L1M25Kind, BaseL1M25 } from "../layer1/module25";
import { computeL0M06, L0M06Kind, BaseL0M06 } from "../layer0/module06";
import { computeL0M19, L0M19Kind, BaseL0M19 } from "../layer0/module19";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L2M09Kind {
	Done,
	Locked,
	Paused,
	Active,
	Failed,
}

export const L2M09_FACTOR = 9;

export class BaseL2M09 {
	protected total = 38;
	public kind = L2M09Kind.Done;

	public warp(value: number): number {
		this.total += value * L2M09_FACTOR;
		return this.total;
	}

	public advance(): L2M09Kind {
		const order = [L2M09Kind.Done, L2M09Kind.Locked, L2M09Kind.Paused, L2M09Kind.Active, L2M09Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL2M09 extends BaseL2M09 {
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

export function computeL2M09(input: number): number {
	let result = input * 9 + 38;
	result += computeL1M25(input % 10);
	result += computeL0M06(input % 6);
	result += computeL0M19(input % 8);
	result += computeL0M01(input % 3);
	return result % 46016;
}

export const L2M09_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 38,
	chain: (value) => computeL2M09(value),
};

export function linkL2M09(): Array<number> {
	const results = new Array<number>();
	const instanceL1M25 = new BaseL1M25();
	instanceL1M25.advance();
	results.push(instanceL1M25.read() + L1M25Kind.Queued);
	const instanceL0M06 = new BaseL0M06();
	instanceL0M06.advance();
	results.push(instanceL0M06.read() + L0M06Kind.Failed);
	const instanceL0M19 = new BaseL0M19();
	instanceL0M19.advance();
	results.push(instanceL0M19.read() + L0M19Kind.Locked);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
