import { computeL1M24, L1M24Kind, BaseL1M24 } from "../layer1/module24";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L2M26Kind {
	Idle,
	Queued,
	Locked,
}

export const L2M26_FACTOR = 19;

export class BaseL2M26 {
	protected total = 82;
	public kind = L2M26Kind.Idle;

	public blend(value: number): number {
		this.total += value * L2M26_FACTOR;
		return this.total;
	}

	public advance(): L2M26Kind {
		const order = [L2M26Kind.Idle, L2M26Kind.Queued, L2M26Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL2M26 extends BaseL1M24 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M26(input: number): number {
	let result = input * 19 + 82;
	result += computeL1M24(input % 11);
	result += computeL0M02(input % 11);
	return result % 31568;
}

export const L2M26_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 82,
	chain: (value) => computeL2M26(value),
};

export function linkL2M26(): Array<number> {
	const results = new Array<number>();
	const instanceL1M24 = new BaseL1M24();
	instanceL1M24.advance();
	results.push(instanceL1M24.read() + L1M24Kind.Active);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
