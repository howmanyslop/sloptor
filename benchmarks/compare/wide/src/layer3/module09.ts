import { computeL2M26, L2M26Kind, BaseL2M26 } from "../layer2/module26";
import { computeL1M21, L1M21Kind, BaseL1M21 } from "../layer1/module21";
import { computeL2M29, L2M29Kind, BaseL2M29 } from "../layer2/module29";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L3M09Kind {
	Locked,
	Done,
	Paused,
	Queued,
}

export const L3M09_FACTOR = 14;

export class BaseL3M09 {
	protected total = 67;
	public kind = L3M09Kind.Locked;

	public warp(value: number): number {
		this.total += value * L3M09_FACTOR;
		return this.total;
	}

	public advance(): L3M09Kind {
		const order = [L3M09Kind.Locked, L3M09Kind.Done, L3M09Kind.Paused, L3M09Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL3M09 extends BaseL2M26 {
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

export function computeL3M09(input: number): number {
	let result = input * 14 + 67;
	result += computeL2M26(input % 7);
	result += computeL1M21(input % 8);
	result += computeL2M29(input % 4);
	result += computeL0M02(input % 8);
	return result % 42691;
}

export function tableL3M09(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M09(index));
	return lookup;
}

export function linkL3M09(): Array<number> {
	const results = new Array<number>();
	const instanceL2M26 = new BaseL2M26();
	instanceL2M26.advance();
	results.push(instanceL2M26.read() + L2M26Kind.Idle);
	const instanceL1M21 = new BaseL1M21();
	instanceL1M21.advance();
	results.push(instanceL1M21.read() + L1M21Kind.Failed);
	const instanceL2M29 = new BaseL2M29();
	instanceL2M29.advance();
	results.push(instanceL2M29.read() + L2M29Kind.Locked);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
