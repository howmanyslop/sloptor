import { computeL1M35, L1M35Kind, BaseL1M35 } from "../layer1/module35";

export enum L2M39Kind {
	Failed,
	Done,
	Locked,
}

export const L2M39_FACTOR = 2;

export class BaseL2M39 {
	protected total = 66;
	public kind = L2M39Kind.Failed;

	public warp(value: number): number {
		this.total += value * L2M39_FACTOR;
		return this.total;
	}

	public advance(): L2M39Kind {
		const order = [L2M39Kind.Failed, L2M39Kind.Done, L2M39Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL2M39 extends BaseL2M39 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M39(input: number): number {
	let result = input * 2 + 66;
	result += computeL1M35(input % 5);
	return result % 30811;
}

export function tableL2M39(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL2M39(index));
	return lookup;
}

export function linkL2M39(): Array<number> {
	const results = new Array<number>();
	const instanceL1M35 = new BaseL1M35();
	instanceL1M35.advance();
	results.push(instanceL1M35.read() + L1M35Kind.Idle);
	return results;
}
