import { computeL4M23, L4M23Kind, BaseL4M23 } from "../layer4/module23";
import { computeL2M17, L2M17Kind, BaseL2M17 } from "../layer2/module17";

export enum L5M14Kind {
	Paused,
	Locked,
	Done,
	Queued,
	Failed,
}

export const L5M14_FACTOR = 19;

export class BaseL5M14 {
	protected total = 47;
	public kind = L5M14Kind.Paused;

	public scale(value: number): number {
		this.total += value * L5M14_FACTOR;
		return this.total;
	}

	public advance(): L5M14Kind {
		const order = [L5M14Kind.Paused, L5M14Kind.Locked, L5M14Kind.Done, L5M14Kind.Queued, L5M14Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL5M14 extends BaseL5M14 {
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

export function computeL5M14(input: number): number {
	let result = input * 19 + 47;
	result += computeL4M23(input % 3);
	result += computeL2M17(input % 8);
	return result % 81298;
}

export const L5M14_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 47,
	chain: (value) => computeL5M14(value),
};

export function linkL5M14(): Array<number> {
	const results = new Array<number>();
	const instanceL4M23 = new BaseL4M23();
	instanceL4M23.advance();
	results.push(instanceL4M23.read() + L4M23Kind.Active);
	const instanceL2M17 = new BaseL2M17();
	instanceL2M17.advance();
	results.push(instanceL2M17.read() + L2M17Kind.Idle);
	return results;
}
