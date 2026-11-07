import { computeL0M11, L0M11Kind, BaseL0M11 } from "../layer0/module11";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L1M30Kind {
	Locked,
	Idle,
	Queued,
}

export const L1M30_FACTOR = 12;

export class BaseL1M30 {
	protected total = 62;
	public kind = L1M30Kind.Locked;

	public blend(value: number): number {
		this.total += value * L1M30_FACTOR;
		return this.total;
	}

	public advance(): L1M30Kind {
		const order = [L1M30Kind.Locked, L1M30Kind.Idle, L1M30Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL1M30 extends BaseL0M11 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 7) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M30(input: number): number {
	let result = input * 12 + 62;
	result += computeL0M11(input % 10);
	result += computeL0M00(input % 10);
	return result % 76216;
}

export function tableL1M30(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M30(index));
	return lookup;
}

export function linkL1M30(): Array<number> {
	const results = new Array<number>();
	const instanceL0M11 = new BaseL0M11();
	instanceL0M11.advance();
	results.push(instanceL0M11.read() + L0M11Kind.Active);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
