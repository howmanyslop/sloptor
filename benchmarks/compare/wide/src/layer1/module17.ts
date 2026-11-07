import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";
import { computeL0M05, L0M05Kind, BaseL0M05 } from "../layer0/module05";
import { computeL0M28, L0M28Kind, BaseL0M28 } from "../layer0/module28";

export enum L1M17Kind {
	Paused,
	Idle,
	Failed,
	Active,
}

export const L1M17_FACTOR = 13;

export class BaseL1M17 {
	protected total = 18;
	public kind = L1M17Kind.Paused;

	public warp(value: number): number {
		this.total += value * L1M17_FACTOR;
		return this.total;
	}

	public advance(): L1M17Kind {
		const order = [L1M17Kind.Paused, L1M17Kind.Idle, L1M17Kind.Failed, L1M17Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL1M17 extends BaseL0M00 {
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

export function computeL1M17(input: number): number {
	let result = input * 13 + 18;
	result += computeL0M00(input % 6);
	result += computeL0M05(input % 10);
	result += computeL0M28(input % 11);
	return result % 17479;
}

export function tableL1M17(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M17(index));
	return lookup;
}

export function linkL1M17(): Array<number> {
	const results = new Array<number>();
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	const instanceL0M05 = new BaseL0M05();
	instanceL0M05.advance();
	results.push(instanceL0M05.read() + L0M05Kind.Done);
	const instanceL0M28 = new BaseL0M28();
	instanceL0M28.advance();
	results.push(instanceL0M28.read() + L0M28Kind.Active);
	return results;
}
