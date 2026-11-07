import { computeL3M29, L3M29Kind, BaseL3M29 } from "../layer3/module29";

export enum L4M15Kind {
	Paused,
	Active,
	Idle,
}

export const L4M15_FACTOR = 6;

export class BaseL4M15 {
	protected total = 27;
	public kind = L4M15Kind.Paused;

	public warp(value: number): number {
		this.total += value * L4M15_FACTOR;
		return this.total;
	}

	public advance(): L4M15Kind {
		const order = [L4M15Kind.Paused, L4M15Kind.Active, L4M15Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL4M15 extends BaseL4M15 {
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

export function computeL4M15(input: number): number {
	let result = input * 6 + 27;
	result += computeL3M29(input % 6);
	return result % 54037;
}

export const L4M15_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 27,
	chain: (value) => computeL4M15(value),
};

export function linkL4M15(): Array<number> {
	const results = new Array<number>();
	const instanceL3M29 = new BaseL3M29();
	instanceL3M29.advance();
	results.push(instanceL3M29.read() + L3M29Kind.Locked);
	return results;
}
