import { computeL0M26, L0M26Kind, BaseL0M26 } from "../layer0/module26";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L1M15Kind {
	Done,
	Idle,
	Queued,
	Active,
	Paused,
}

export const L1M15_FACTOR = 10;

export class BaseL1M15 {
	protected total = 87;
	public kind = L1M15Kind.Done;

	public tally(value: number): number {
		this.total += value * L1M15_FACTOR;
		return this.total;
	}

	public advance(): L1M15Kind {
		const order = [L1M15Kind.Done, L1M15Kind.Idle, L1M15Kind.Queued, L1M15Kind.Active, L1M15Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL1M15 extends BaseL0M26 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M15(input: number): number {
	let result = input * 10 + 87;
	result += computeL0M26(input % 10);
	result += computeL0M00(input % 9);
	return result % 10771;
}

export function tableL1M15(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M15(index));
	return lookup;
}

export function linkL1M15(): Array<number> {
	const results = new Array<number>();
	const instanceL0M26 = new BaseL0M26();
	instanceL0M26.advance();
	results.push(instanceL0M26.read() + L0M26Kind.Idle);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
