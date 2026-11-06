import { computeL1M26, L1M26Kind, BaseL1M26 } from "../layer1/module26";
import { computeL0M07, L0M07Kind, BaseL0M07 } from "../layer0/module07";
import { computeL1M13, L1M13Kind, BaseL1M13 } from "../layer1/module13";

export enum L2M32Kind {
	Active,
	Failed,
	Done,
	Idle,
}

export const L2M32_FACTOR = 4;

export class BaseL2M32 {
	protected total = 8;
	public kind = L2M32Kind.Active;

	public warp(value: number): number {
		this.total += value * L2M32_FACTOR;
		return this.total;
	}

	public advance(): L2M32Kind {
		const order = [L2M32Kind.Active, L2M32Kind.Failed, L2M32Kind.Done, L2M32Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL2M32 extends BaseL1M26 {
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

export function computeL2M32(input: number): number {
	let result = input * 4 + 8;
	result += computeL1M26(input % 4);
	result += computeL0M07(input % 3);
	result += computeL1M13(input % 3);
	return result % 49542;
}

export function tableL2M32(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL2M32(index));
	return lookup;
}

export function linkL2M32(): Array<number> {
	const results = new Array<number>();
	const instanceL1M26 = new BaseL1M26();
	instanceL1M26.advance();
	results.push(instanceL1M26.read() + L1M26Kind.Locked);
	const instanceL0M07 = new BaseL0M07();
	instanceL0M07.advance();
	results.push(instanceL0M07.read() + L0M07Kind.Idle);
	const instanceL1M13 = new BaseL1M13();
	instanceL1M13.advance();
	results.push(instanceL1M13.read() + L1M13Kind.Idle);
	return results;
}
