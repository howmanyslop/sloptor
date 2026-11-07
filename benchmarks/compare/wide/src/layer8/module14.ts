import { computeL7M26, L7M26Kind, BaseL7M26 } from "../layer7/module26";

export enum L8M14Kind {
	Failed,
	Active,
	Done,
	Idle,
}

export const L8M14_FACTOR = 15;

export class BaseL8M14 {
	protected total = 24;
	public kind = L8M14Kind.Failed;

	public tally(value: number): number {
		this.total += value * L8M14_FACTOR;
		return this.total;
	}

	public advance(): L8M14Kind {
		const order = [L8M14Kind.Failed, L8M14Kind.Active, L8M14Kind.Done, L8M14Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL8M14 extends BaseL8M14 {
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

export function computeL8M14(input: number): number {
	let result = input * 15 + 24;
	result += computeL7M26(input % 10);
	return result % 46683;
}

export const L8M14_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 24,
	chain: (value) => computeL8M14(value),
};

export function linkL8M14(): Array<number> {
	const results = new Array<number>();
	const instanceL7M26 = new BaseL7M26();
	instanceL7M26.advance();
	results.push(instanceL7M26.read() + L7M26Kind.Locked);
	return results;
}
