import { computeL4M21, L4M21Kind, BaseL4M21 } from "../layer4/module21";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L5M08Kind {
	Idle,
	Locked,
	Done,
}

export const L5M08_FACTOR = 16;

export class BaseL5M08 {
	protected total = 11;
	public kind = L5M08Kind.Idle;

	public shift(value: number): number {
		this.total += value * L5M08_FACTOR;
		return this.total;
	}

	public advance(): L5M08Kind {
		const order = [L5M08Kind.Idle, L5M08Kind.Locked, L5M08Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL5M08 extends BaseL4M21 {
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

export function computeL5M08(input: number): number {
	let result = input * 16 + 11;
	result += computeL4M21(input % 3);
	result += computeL0M01(input % 6);
	return result % 54737;
}

export const L5M08_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 11,
	chain: (value) => computeL5M08(value),
};

export function linkL5M08(): Array<number> {
	const results = new Array<number>();
	const instanceL4M21 = new BaseL4M21();
	instanceL4M21.advance();
	results.push(instanceL4M21.read() + L4M21Kind.Paused);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
