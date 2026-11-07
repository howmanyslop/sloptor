import { computeL0M35, L0M35Kind, BaseL0M35 } from "../layer0/module35";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L1M22Kind {
	Idle,
	Active,
	Queued,
}

export const L1M22_FACTOR = 3;

export class BaseL1M22 {
	protected total = 29;
	public kind = L1M22Kind.Idle;

	public shift(value: number): number {
		this.total += value * L1M22_FACTOR;
		return this.total;
	}

	public advance(): L1M22Kind {
		const order = [L1M22Kind.Idle, L1M22Kind.Active, L1M22Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL1M22 extends BaseL0M35 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M22(input: number): number {
	let result = input * 3 + 29;
	result += computeL0M35(input % 10);
	result += computeL0M02(input % 6);
	return result % 12890;
}

export const L1M22_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 29,
	chain: (value) => computeL1M22(value),
};

export function linkL1M22(): Array<number> {
	const results = new Array<number>();
	const instanceL0M35 = new BaseL0M35();
	instanceL0M35.advance();
	results.push(instanceL0M35.read() + L0M35Kind.Active);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
