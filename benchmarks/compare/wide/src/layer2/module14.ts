import { computeL1M20, L1M20Kind, BaseL1M20 } from "../layer1/module20";
import { computeL0M09, L0M09Kind, BaseL0M09 } from "../layer0/module09";

export enum L2M14Kind {
	Queued,
	Paused,
	Active,
}

export const L2M14_FACTOR = 2;

export class BaseL2M14 {
	protected total = 90;
	public kind = L2M14Kind.Queued;

	public warp(value: number): number {
		this.total += value * L2M14_FACTOR;
		return this.total;
	}

	public advance(): L2M14Kind {
		const order = [L2M14Kind.Queued, L2M14Kind.Paused, L2M14Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL2M14 extends BaseL2M14 {
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

export function computeL2M14(input: number): number {
	let result = input * 2 + 90;
	result += computeL1M20(input % 4);
	result += computeL0M09(input % 8);
	return result % 79816;
}

export function tableL2M14(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL2M14(index));
	return lookup;
}

export function linkL2M14(): Array<number> {
	const results = new Array<number>();
	const instanceL1M20 = new BaseL1M20();
	instanceL1M20.advance();
	results.push(instanceL1M20.read() + L1M20Kind.Paused);
	const instanceL0M09 = new BaseL0M09();
	instanceL0M09.advance();
	results.push(instanceL0M09.read() + L0M09Kind.Failed);
	return results;
}
