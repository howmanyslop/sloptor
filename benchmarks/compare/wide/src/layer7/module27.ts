import { computeL6M36, L6M36Kind, BaseL6M36 } from "../layer6/module36";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L7M27Kind {
	Queued,
	Failed,
	Done,
	Paused,
}

export const L7M27_FACTOR = 8;

export class BaseL7M27 {
	protected total = 77;
	public kind = L7M27Kind.Queued;

	public warp(value: number): number {
		this.total += value * L7M27_FACTOR;
		return this.total;
	}

	public advance(): L7M27Kind {
		const order = [L7M27Kind.Queued, L7M27Kind.Failed, L7M27Kind.Done, L7M27Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M27(input: number): number {
	let result = input * 8 + 77;
	result += computeL6M36(input % 3);
	result += computeL0M02(input % 4);
	return result % 63891;
}

export function tableL7M27(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M27(index));
	return lookup;
}

export function linkL7M27(): Array<number> {
	const results = new Array<number>();
	const instanceL6M36 = new BaseL6M36();
	instanceL6M36.advance();
	results.push(instanceL6M36.read() + L6M36Kind.Queued);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
