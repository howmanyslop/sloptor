import { computeL5M26, L5M26Kind, BaseL5M26 } from "../layer5/module26";
import { computeL4M20, L4M20Kind, BaseL4M20 } from "../layer4/module20";
import { computeL5M28, L5M28Kind, BaseL5M28 } from "../layer5/module28";

export enum L6M20Kind {
	Failed,
	Paused,
	Locked,
	Queued,
	Active,
}

export const L6M20_FACTOR = 15;

export class BaseL6M20 {
	protected total = 33;
	public kind = L6M20Kind.Failed;

	public warp(value: number): number {
		this.total += value * L6M20_FACTOR;
		return this.total;
	}

	public advance(): L6M20Kind {
		const order = [L6M20Kind.Failed, L6M20Kind.Paused, L6M20Kind.Locked, L6M20Kind.Queued, L6M20Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL6M20 extends BaseL5M26 {
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

export function computeL6M20(input: number): number {
	let result = input * 15 + 33;
	result += computeL5M26(input % 7);
	result += computeL4M20(input % 8);
	result += computeL5M28(input % 11);
	return result % 78337;
}

export function tableL6M20(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M20(index));
	return lookup;
}

export function linkL6M20(): Array<number> {
	const results = new Array<number>();
	const instanceL5M26 = new BaseL5M26();
	instanceL5M26.advance();
	results.push(instanceL5M26.read() + L5M26Kind.Locked);
	const instanceL4M20 = new BaseL4M20();
	instanceL4M20.advance();
	results.push(instanceL4M20.read() + L4M20Kind.Done);
	const instanceL5M28 = new BaseL5M28();
	instanceL5M28.advance();
	results.push(instanceL5M28.read() + L5M28Kind.Active);
	return results;
}
