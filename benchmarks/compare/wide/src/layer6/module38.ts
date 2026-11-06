import { computeL5M32, L5M32Kind, BaseL5M32 } from "../layer5/module32";
import { computeL5M01, L5M01Kind, BaseL5M01 } from "../layer5/module01";

export enum L6M38Kind {
	Done,
	Idle,
	Paused,
}

export const L6M38_FACTOR = 2;

export class BaseL6M38 {
	protected total = 59;
	public kind = L6M38Kind.Done;

	public scale(value: number): number {
		this.total += value * L6M38_FACTOR;
		return this.total;
	}

	public advance(): L6M38Kind {
		const order = [L6M38Kind.Done, L6M38Kind.Idle, L6M38Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL6M38 extends BaseL5M32 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 14) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M38(input: number): number {
	let result = input * 2 + 59;
	result += computeL5M32(input % 3);
	result += computeL5M01(input % 7);
	return result % 71115;
}

export function tableL6M38(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M38(index));
	return lookup;
}

export function linkL6M38(): Array<number> {
	const results = new Array<number>();
	const instanceL5M32 = new BaseL5M32();
	instanceL5M32.advance();
	results.push(instanceL5M32.read() + L5M32Kind.Queued);
	const instanceL5M01 = new BaseL5M01();
	instanceL5M01.advance();
	results.push(instanceL5M01.read() + L5M01Kind.Idle);
	return results;
}
