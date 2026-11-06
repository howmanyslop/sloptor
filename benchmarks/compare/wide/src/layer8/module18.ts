import { computeL7M14, L7M14Kind, BaseL7M14 } from "../layer7/module14";

export enum L8M18Kind {
	Done,
	Paused,
	Queued,
	Failed,
}

export const L8M18_FACTOR = 8;

export class BaseL8M18 {
	protected total = 7;
	public kind = L8M18Kind.Done;

	public warp(value: number): number {
		this.total += value * L8M18_FACTOR;
		return this.total;
	}

	public advance(): L8M18Kind {
		const order = [L8M18Kind.Done, L8M18Kind.Paused, L8M18Kind.Queued, L8M18Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL8M18 extends BaseL7M14 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M18(input: number): number {
	let result = input * 8 + 7;
	result += computeL7M14(input % 5);
	return result % 70055;
}

export const L8M18_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 7,
	chain: (value) => computeL8M18(value),
};

export function linkL8M18(): Array<number> {
	const results = new Array<number>();
	const instanceL7M14 = new BaseL7M14();
	instanceL7M14.advance();
	results.push(instanceL7M14.read() + L7M14Kind.Idle);
	return results;
}
