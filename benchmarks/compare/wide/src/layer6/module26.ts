import { computeL5M28, L5M28Kind, BaseL5M28 } from "../layer5/module28";
import { computeL1M39, L1M39Kind, BaseL1M39 } from "../layer1/module39";
import { computeL5M25, L5M25Kind, BaseL5M25 } from "../layer5/module25";

export enum L6M26Kind {
	Locked,
	Done,
	Idle,
	Queued,
	Active,
}

export const L6M26_FACTOR = 13;

export class BaseL6M26 {
	protected total = 27;
	public kind = L6M26Kind.Locked;

	public warp(value: number): number {
		this.total += value * L6M26_FACTOR;
		return this.total;
	}

	public advance(): L6M26Kind {
		const order = [L6M26Kind.Locked, L6M26Kind.Done, L6M26Kind.Idle, L6M26Kind.Queued, L6M26Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL6M26 extends BaseL6M26 {
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

export function computeL6M26(input: number): number {
	let result = input * 13 + 27;
	result += computeL5M28(input % 5);
	result += computeL1M39(input % 6);
	result += computeL5M25(input % 7);
	return result % 46842;
}

export const L6M26_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 27,
	chain: (value) => computeL6M26(value),
};

export function linkL6M26(): Array<number> {
	const results = new Array<number>();
	const instanceL5M28 = new BaseL5M28();
	instanceL5M28.advance();
	results.push(instanceL5M28.read() + L5M28Kind.Active);
	const instanceL1M39 = new BaseL1M39();
	instanceL1M39.advance();
	results.push(instanceL1M39.read() + L1M39Kind.Done);
	const instanceL5M25 = new BaseL5M25();
	instanceL5M25.advance();
	results.push(instanceL5M25.read() + L5M25Kind.Paused);
	return results;
}
