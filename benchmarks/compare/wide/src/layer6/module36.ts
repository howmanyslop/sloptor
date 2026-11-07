import { computeL5M18, L5M18Kind, BaseL5M18 } from "../layer5/module18";
import { computeL5M33, L5M33Kind, BaseL5M33 } from "../layer5/module33";
import { computeL0M13, L0M13Kind, BaseL0M13 } from "../layer0/module13";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L6M36Kind {
	Queued,
	Done,
	Active,
}

export const L6M36_FACTOR = 9;

export class BaseL6M36 {
	protected total = 13;
	public kind = L6M36Kind.Queued;

	public warp(value: number): number {
		this.total += value * L6M36_FACTOR;
		return this.total;
	}

	public advance(): L6M36Kind {
		const order = [L6M36Kind.Queued, L6M36Kind.Done, L6M36Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL6M36 extends BaseL6M36 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M36(input: number): number {
	let result = input * 9 + 13;
	result += computeL5M18(input % 4);
	result += computeL5M33(input % 3);
	result += computeL0M13(input % 10);
	result += computeL0M02(input % 10);
	return result % 90442;
}

export function tableL6M36(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M36(index));
	return lookup;
}

export function linkL6M36(): Array<number> {
	const results = new Array<number>();
	const instanceL5M18 = new BaseL5M18();
	instanceL5M18.advance();
	results.push(instanceL5M18.read() + L5M18Kind.Locked);
	const instanceL5M33 = new BaseL5M33();
	instanceL5M33.advance();
	results.push(instanceL5M33.read() + L5M33Kind.Idle);
	const instanceL0M13 = new BaseL0M13();
	instanceL0M13.advance();
	results.push(instanceL0M13.read() + L0M13Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
