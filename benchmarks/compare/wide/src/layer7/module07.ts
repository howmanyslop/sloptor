import { computeL6M16, L6M16Kind, BaseL6M16 } from "../layer6/module16";
import { computeL0M13, L0M13Kind, BaseL0M13 } from "../layer0/module13";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L7M07Kind {
	Done,
	Idle,
	Active,
	Failed,
	Paused,
}

export const L7M07_FACTOR = 12;

export class BaseL7M07 {
	protected total = 60;
	public kind = L7M07Kind.Done;

	public weigh(value: number): number {
		this.total += value * L7M07_FACTOR;
		return this.total;
	}

	public advance(): L7M07Kind {
		const order = [L7M07Kind.Done, L7M07Kind.Idle, L7M07Kind.Active, L7M07Kind.Failed, L7M07Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M07(input: number): number {
	let result = input * 12 + 60;
	result += computeL6M16(input % 3);
	result += computeL0M13(input % 7);
	result += computeL0M02(input % 9);
	return result % 33217;
}

export function tableL7M07(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M07(index));
	return lookup;
}

export function linkL7M07(): Array<number> {
	const results = new Array<number>();
	const instanceL6M16 = new BaseL6M16();
	instanceL6M16.advance();
	results.push(instanceL6M16.read() + L6M16Kind.Idle);
	const instanceL0M13 = new BaseL0M13();
	instanceL0M13.advance();
	results.push(instanceL0M13.read() + L0M13Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
