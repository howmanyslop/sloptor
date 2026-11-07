import { computeL6M37, L6M37Kind, BaseL6M37 } from "../layer6/module37";
import { computeL1M05, L1M05Kind, BaseL1M05 } from "../layer1/module05";

export enum L7M10Kind {
	Locked,
	Failed,
	Done,
}

export const L7M10_FACTOR = 19;

export class BaseL7M10 {
	protected total = 90;
	public kind = L7M10Kind.Locked;

	public mix(value: number): number {
		this.total += value * L7M10_FACTOR;
		return this.total;
	}

	public advance(): L7M10Kind {
		const order = [L7M10Kind.Locked, L7M10Kind.Failed, L7M10Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL7M10 extends BaseL7M10 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M10(input: number): number {
	let result = input * 19 + 90;
	result += computeL6M37(input % 5);
	result += computeL1M05(input % 6);
	return result % 52435;
}

export const L7M10_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 90,
	chain: (value) => computeL7M10(value),
};

export function linkL7M10(): Array<number> {
	const results = new Array<number>();
	const instanceL6M37 = new BaseL6M37();
	instanceL6M37.advance();
	results.push(instanceL6M37.read() + L6M37Kind.Active);
	const instanceL1M05 = new BaseL1M05();
	instanceL1M05.advance();
	results.push(instanceL1M05.read() + L1M05Kind.Idle);
	return results;
}
