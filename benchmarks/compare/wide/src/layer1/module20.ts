import { computeL0M21, L0M21Kind, BaseL0M21 } from "../layer0/module21";
import { computeL0M14, L0M14Kind, BaseL0M14 } from "../layer0/module14";

export enum L1M20Kind {
	Paused,
	Done,
	Active,
}

export const L1M20_FACTOR = 10;

export class BaseL1M20 {
	protected total = 38;
	public kind = L1M20Kind.Paused;

	public shift(value: number): number {
		this.total += value * L1M20_FACTOR;
		return this.total;
	}

	public advance(): L1M20Kind {
		const order = [L1M20Kind.Paused, L1M20Kind.Done, L1M20Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL1M20 extends BaseL0M21 {
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

export function computeL1M20(input: number): number {
	let result = input * 10 + 38;
	result += computeL0M21(input % 10);
	result += computeL0M14(input % 7);
	return result % 26313;
}

export function tableL1M20(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M20(index));
	return lookup;
}

export function linkL1M20(): Array<number> {
	const results = new Array<number>();
	const instanceL0M21 = new BaseL0M21();
	instanceL0M21.advance();
	results.push(instanceL0M21.read() + L0M21Kind.Done);
	const instanceL0M14 = new BaseL0M14();
	instanceL0M14.advance();
	results.push(instanceL0M14.read() + L0M14Kind.Idle);
	return results;
}
