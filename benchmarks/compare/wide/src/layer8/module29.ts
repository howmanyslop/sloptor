import { computeL7M11, L7M11Kind, BaseL7M11 } from "../layer7/module11";
import { computeL1M20, L1M20Kind, BaseL1M20 } from "../layer1/module20";
import { computeL2M39, L2M39Kind, BaseL2M39 } from "../layer2/module39";

export enum L8M29Kind {
	Idle,
	Active,
	Queued,
	Paused,
	Done,
}

export const L8M29_FACTOR = 2;

export class BaseL8M29 {
	protected total = 43;
	public kind = L8M29Kind.Idle;

	public fold(value: number): number {
		this.total += value * L8M29_FACTOR;
		return this.total;
	}

	public advance(): L8M29Kind {
		const order = [L8M29Kind.Idle, L8M29Kind.Active, L8M29Kind.Queued, L8M29Kind.Paused, L8M29Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL8M29 extends BaseL7M11 {
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

export function computeL8M29(input: number): number {
	let result = input * 2 + 43;
	result += computeL7M11(input % 5);
	result += computeL1M20(input % 5);
	result += computeL2M39(input % 8);
	return result % 14383;
}

export const L8M29_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 43,
	chain: (value) => computeL8M29(value),
};

export function linkL8M29(): Array<number> {
	const results = new Array<number>();
	const instanceL7M11 = new BaseL7M11();
	instanceL7M11.advance();
	results.push(instanceL7M11.read() + L7M11Kind.Done);
	const instanceL1M20 = new BaseL1M20();
	instanceL1M20.advance();
	results.push(instanceL1M20.read() + L1M20Kind.Paused);
	const instanceL2M39 = new BaseL2M39();
	instanceL2M39.advance();
	results.push(instanceL2M39.read() + L2M39Kind.Failed);
	return results;
}
