import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";
import { computeL0M04, L0M04Kind, BaseL0M04 } from "../layer0/module04";

export enum L1M00Kind {
	Paused,
	Locked,
	Active,
	Idle,
}

export const L1M00_FACTOR = 8;

export class BaseL1M00 {
	protected total = 41;
	public kind = L1M00Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L1M00_FACTOR;
		return this.total;
	}

	public advance(): L1M00Kind {
		const order = [L1M00Kind.Paused, L1M00Kind.Locked, L1M00Kind.Active, L1M00Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL1M00 extends BaseL1M00 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 10) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M00(input: number): number {
	let result = input * 8 + 41;
	result += computeL0M01(input % 11);
	result += computeL0M04(input % 8);
	return result % 34422;
}

export const L1M00_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 41,
	chain: (value) => computeL1M00(value),
};

export function linkL1M00(): Array<number> {
	const results = new Array<number>();
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	const instanceL0M04 = new BaseL0M04();
	instanceL0M04.advance();
	results.push(instanceL0M04.read() + L0M04Kind.Paused);
	return results;
}
