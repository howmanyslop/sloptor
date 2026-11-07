import { computeL6M21, L6M21Kind, BaseL6M21 } from "../layer6/module21";
import { computeL0M27, L0M27Kind, BaseL0M27 } from "../layer0/module27";
import { computeL5M14, L5M14Kind, BaseL5M14 } from "../layer5/module14";

export enum L7M02Kind {
	Queued,
	Failed,
	Done,
}

export const L7M02_FACTOR = 4;

export class BaseL7M02 {
	protected total = 83;
	public kind = L7M02Kind.Queued;

	public blend(value: number): number {
		this.total += value * L7M02_FACTOR;
		return this.total;
	}

	public advance(): L7M02Kind {
		const order = [L7M02Kind.Queued, L7M02Kind.Failed, L7M02Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL7M02 extends BaseL6M21 {
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

export function computeL7M02(input: number): number {
	let result = input * 4 + 83;
	result += computeL6M21(input % 5);
	result += computeL0M27(input % 5);
	result += computeL5M14(input % 10);
	return result % 40495;
}

export function tableL7M02(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M02(index));
	return lookup;
}

export function linkL7M02(): Array<number> {
	const results = new Array<number>();
	const instanceL6M21 = new BaseL6M21();
	instanceL6M21.advance();
	results.push(instanceL6M21.read() + L6M21Kind.Queued);
	const instanceL0M27 = new BaseL0M27();
	instanceL0M27.advance();
	results.push(instanceL0M27.read() + L0M27Kind.Done);
	const instanceL5M14 = new BaseL5M14();
	instanceL5M14.advance();
	results.push(instanceL5M14.read() + L5M14Kind.Paused);
	return results;
}
