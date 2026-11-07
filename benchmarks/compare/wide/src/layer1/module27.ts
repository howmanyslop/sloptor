import { computeL0M30, L0M30Kind, BaseL0M30 } from "../layer0/module30";
import { computeL0M25, L0M25Kind, BaseL0M25 } from "../layer0/module25";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L1M27Kind {
	Paused,
	Failed,
	Done,
	Active,
}

export const L1M27_FACTOR = 16;

export class BaseL1M27 {
	protected total = 14;
	public kind = L1M27Kind.Paused;

	public blend(value: number): number {
		this.total += value * L1M27_FACTOR;
		return this.total;
	}

	public advance(): L1M27Kind {
		const order = [L1M27Kind.Paused, L1M27Kind.Failed, L1M27Kind.Done, L1M27Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL1M27 extends BaseL0M30 {
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

export function computeL1M27(input: number): number {
	let result = input * 16 + 14;
	result += computeL0M30(input % 10);
	result += computeL0M25(input % 3);
	result += computeL0M02(input % 5);
	return result % 20607;
}

export const L1M27_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 14,
	chain: (value) => computeL1M27(value),
};

export function linkL1M27(): Array<number> {
	const results = new Array<number>();
	const instanceL0M30 = new BaseL0M30();
	instanceL0M30.advance();
	results.push(instanceL0M30.read() + L0M30Kind.Done);
	const instanceL0M25 = new BaseL0M25();
	instanceL0M25.advance();
	results.push(instanceL0M25.read() + L0M25Kind.Done);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
