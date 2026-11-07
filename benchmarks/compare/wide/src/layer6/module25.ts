import { computeL5M21, L5M21Kind, BaseL5M21 } from "../layer5/module21";
import { computeL3M39, L3M39Kind, BaseL3M39 } from "../layer3/module39";
import { computeL2M09, L2M09Kind, BaseL2M09 } from "../layer2/module09";

export enum L6M25Kind {
	Paused,
	Done,
	Queued,
}

export const L6M25_FACTOR = 12;

export class BaseL6M25 {
	protected total = 47;
	public kind = L6M25Kind.Paused;

	public tally(value: number): number {
		this.total += value * L6M25_FACTOR;
		return this.total;
	}

	public advance(): L6M25Kind {
		const order = [L6M25Kind.Paused, L6M25Kind.Done, L6M25Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL6M25 extends BaseL6M25 {
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

export function computeL6M25(input: number): number {
	let result = input * 12 + 47;
	result += computeL5M21(input % 6);
	result += computeL3M39(input % 11);
	result += computeL2M09(input % 11);
	return result % 14847;
}

export const L6M25_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 47,
	chain: (value) => computeL6M25(value),
};

export function linkL6M25(): Array<number> {
	const results = new Array<number>();
	const instanceL5M21 = new BaseL5M21();
	instanceL5M21.advance();
	results.push(instanceL5M21.read() + L5M21Kind.Idle);
	const instanceL3M39 = new BaseL3M39();
	instanceL3M39.advance();
	results.push(instanceL3M39.read() + L3M39Kind.Done);
	const instanceL2M09 = new BaseL2M09();
	instanceL2M09.advance();
	results.push(instanceL2M09.read() + L2M09Kind.Done);
	return results;
}
