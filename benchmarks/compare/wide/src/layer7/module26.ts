import { computeL6M29, L6M29Kind, BaseL6M29 } from "../layer6/module29";

export enum L7M26Kind {
	Locked,
	Queued,
	Failed,
}

export const L7M26_FACTOR = 6;

export class BaseL7M26 {
	protected total = 67;
	public kind = L7M26Kind.Locked;

	public blend(value: number): number {
		this.total += value * L7M26_FACTOR;
		return this.total;
	}

	public advance(): L7M26Kind {
		const order = [L7M26Kind.Locked, L7M26Kind.Queued, L7M26Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL7M26 extends BaseL7M26 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M26(input: number): number {
	let result = input * 6 + 67;
	result += computeL6M29(input % 11);
	return result % 33336;
}

export function tableL7M26(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M26(index));
	return lookup;
}

export function linkL7M26(): Array<number> {
	const results = new Array<number>();
	const instanceL6M29 = new BaseL6M29();
	instanceL6M29.advance();
	results.push(instanceL6M29.read() + L6M29Kind.Failed);
	return results;
}
