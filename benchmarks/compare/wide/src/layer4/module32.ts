import { computeL3M28, L3M28Kind, BaseL3M28 } from "../layer3/module28";
import { computeL0M34, L0M34Kind, BaseL0M34 } from "../layer0/module34";

export enum L4M32Kind {
	Paused,
	Idle,
	Done,
	Locked,
}

export const L4M32_FACTOR = 11;

export class BaseL4M32 {
	protected total = 48;
	public kind = L4M32Kind.Paused;

	public scale(value: number): number {
		this.total += value * L4M32_FACTOR;
		return this.total;
	}

	public advance(): L4M32Kind {
		const order = [L4M32Kind.Paused, L4M32Kind.Idle, L4M32Kind.Done, L4M32Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL4M32 extends BaseL3M28 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M32(input: number): number {
	let result = input * 11 + 48;
	result += computeL3M28(input % 4);
	result += computeL0M34(input % 11);
	return result % 78976;
}

export function tableL4M32(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M32(index));
	return lookup;
}

export function linkL4M32(): Array<number> {
	const results = new Array<number>();
	const instanceL3M28 = new BaseL3M28();
	instanceL3M28.advance();
	results.push(instanceL3M28.read() + L3M28Kind.Idle);
	const instanceL0M34 = new BaseL0M34();
	instanceL0M34.advance();
	results.push(instanceL0M34.read() + L0M34Kind.Locked);
	return results;
}
