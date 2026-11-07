import { computeL4M38, L4M38Kind, BaseL4M38 } from "../layer4/module38";
import { computeL0M21, L0M21Kind, BaseL0M21 } from "../layer0/module21";

export enum L5M39Kind {
	Done,
	Queued,
	Paused,
}

export const L5M39_FACTOR = 14;

export class BaseL5M39 {
	protected total = 38;
	public kind = L5M39Kind.Done;

	public shift(value: number): number {
		this.total += value * L5M39_FACTOR;
		return this.total;
	}

	public advance(): L5M39Kind {
		const order = [L5M39Kind.Done, L5M39Kind.Queued, L5M39Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL5M39 extends BaseL4M38 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 11) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL5M39(input: number): number {
	let result = input * 14 + 38;
	result += computeL4M38(input % 5);
	result += computeL0M21(input % 9);
	return result % 19038;
}

export function tableL5M39(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M39(index));
	return lookup;
}

export function linkL5M39(): Array<number> {
	const results = new Array<number>();
	const instanceL4M38 = new BaseL4M38();
	instanceL4M38.advance();
	results.push(instanceL4M38.read() + L4M38Kind.Failed);
	const instanceL0M21 = new BaseL0M21();
	instanceL0M21.advance();
	results.push(instanceL0M21.read() + L0M21Kind.Done);
	return results;
}
