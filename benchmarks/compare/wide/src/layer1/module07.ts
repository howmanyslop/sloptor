import { computeL0M10, L0M10Kind, BaseL0M10 } from "../layer0/module10";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L1M07Kind {
	Idle,
	Active,
	Locked,
	Queued,
}

export const L1M07_FACTOR = 5;

export class BaseL1M07 {
	protected total = 57;
	public kind = L1M07Kind.Idle;

	public tally(value: number): number {
		this.total += value * L1M07_FACTOR;
		return this.total;
	}

	public advance(): L1M07Kind {
		const order = [L1M07Kind.Idle, L1M07Kind.Active, L1M07Kind.Locked, L1M07Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL1M07 extends BaseL1M07 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 16) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M07(input: number): number {
	let result = input * 5 + 57;
	result += computeL0M10(input % 6);
	result += computeL0M02(input % 11);
	return result % 31201;
}

export const L1M07_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 57,
	chain: (value) => computeL1M07(value),
};

export function linkL1M07(): Array<number> {
	const results = new Array<number>();
	const instanceL0M10 = new BaseL0M10();
	instanceL0M10.advance();
	results.push(instanceL0M10.read() + L0M10Kind.Active);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
