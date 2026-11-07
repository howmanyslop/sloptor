import { computeL6M03, L6M03Kind, BaseL6M03 } from "../layer6/module03";
import { computeL2M25, L2M25Kind, BaseL2M25 } from "../layer2/module25";

export enum L7M28Kind {
	Queued,
	Idle,
	Paused,
}

export const L7M28_FACTOR = 7;

export class BaseL7M28 {
	protected total = 41;
	public kind = L7M28Kind.Queued;

	public shift(value: number): number {
		this.total += value * L7M28_FACTOR;
		return this.total;
	}

	public advance(): L7M28Kind {
		const order = [L7M28Kind.Queued, L7M28Kind.Idle, L7M28Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL7M28 extends BaseL7M28 {
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

export function computeL7M28(input: number): number {
	let result = input * 7 + 41;
	result += computeL6M03(input % 6);
	result += computeL2M25(input % 10);
	return result % 5417;
}

export const L7M28_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 41,
	chain: (value) => computeL7M28(value),
};

export function linkL7M28(): Array<number> {
	const results = new Array<number>();
	const instanceL6M03 = new BaseL6M03();
	instanceL6M03.advance();
	results.push(instanceL6M03.read() + L6M03Kind.Locked);
	const instanceL2M25 = new BaseL2M25();
	instanceL2M25.advance();
	results.push(instanceL2M25.read() + L2M25Kind.Idle);
	return results;
}
