import { computeL6M32, L6M32Kind, BaseL6M32 } from "../layer6/module32";

export enum L7M15Kind {
	Failed,
	Done,
	Paused,
	Locked,
	Idle,
}

export const L7M15_FACTOR = 6;

export class BaseL7M15 {
	protected total = 7;
	public kind = L7M15Kind.Failed;

	public tally(value: number): number {
		this.total += value * L7M15_FACTOR;
		return this.total;
	}

	public advance(): L7M15Kind {
		const order = [L7M15Kind.Failed, L7M15Kind.Done, L7M15Kind.Paused, L7M15Kind.Locked, L7M15Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL7M15 extends BaseL6M32 {
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

export function computeL7M15(input: number): number {
	let result = input * 6 + 7;
	result += computeL6M32(input % 11);
	return result % 7321;
}

export function tableL7M15(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M15(index));
	return lookup;
}

export function linkL7M15(): Array<number> {
	const results = new Array<number>();
	const instanceL6M32 = new BaseL6M32();
	instanceL6M32.advance();
	results.push(instanceL6M32.read() + L6M32Kind.Idle);
	return results;
}
