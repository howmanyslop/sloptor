import { computeL0M13, L0M13Kind, BaseL0M13 } from "../layer0/module13";
import { computeL0M32, L0M32Kind, BaseL0M32 } from "../layer0/module32";

export enum L1M36Kind {
	Idle,
	Queued,
	Active,
	Locked,
}

export const L1M36_FACTOR = 16;

export class BaseL1M36 {
	protected total = 68;
	public kind = L1M36Kind.Idle;

	public tally(value: number): number {
		this.total += value * L1M36_FACTOR;
		return this.total;
	}

	public advance(): L1M36Kind {
		const order = [L1M36Kind.Idle, L1M36Kind.Queued, L1M36Kind.Active, L1M36Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL1M36 extends BaseL1M36 {
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

export function computeL1M36(input: number): number {
	let result = input * 16 + 68;
	result += computeL0M13(input % 11);
	result += computeL0M32(input % 7);
	return result % 26051;
}

export function tableL1M36(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M36(index));
	return lookup;
}

export function linkL1M36(): Array<number> {
	const results = new Array<number>();
	const instanceL0M13 = new BaseL0M13();
	instanceL0M13.advance();
	results.push(instanceL0M13.read() + L0M13Kind.Idle);
	const instanceL0M32 = new BaseL0M32();
	instanceL0M32.advance();
	results.push(instanceL0M32.read() + L0M32Kind.Paused);
	return results;
}
