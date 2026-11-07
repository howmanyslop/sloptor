import { computeL4M05, L4M05Kind, BaseL4M05 } from "../layer4/module05";
import { computeL0M36, L0M36Kind, BaseL0M36 } from "../layer0/module36";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L5M00Kind {
	Queued,
	Locked,
	Failed,
	Done,
	Paused,
}

export const L5M00_FACTOR = 3;

export class BaseL5M00 {
	protected total = 13;
	public kind = L5M00Kind.Queued;

	public scale(value: number): number {
		this.total += value * L5M00_FACTOR;
		return this.total;
	}

	public advance(): L5M00Kind {
		const order = [L5M00Kind.Queued, L5M00Kind.Locked, L5M00Kind.Failed, L5M00Kind.Done, L5M00Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M00(input: number): number {
	let result = input * 3 + 13;
	result += computeL4M05(input % 8);
	result += computeL0M36(input % 7);
	result += computeL0M03(input % 6);
	return result % 49039;
}

export function tableL5M00(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M00(index));
	return lookup;
}

export function linkL5M00(): Array<number> {
	const results = new Array<number>();
	const instanceL4M05 = new BaseL4M05();
	instanceL4M05.advance();
	results.push(instanceL4M05.read() + L4M05Kind.Idle);
	const instanceL0M36 = new BaseL0M36();
	instanceL0M36.advance();
	results.push(instanceL0M36.read() + L0M36Kind.Failed);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
