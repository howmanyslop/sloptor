import { computeL2M21, L2M21Kind, BaseL2M21 } from "../layer2/module21";
import { computeL2M24, L2M24Kind, BaseL2M24 } from "../layer2/module24";

export enum L3M14Kind {
	Idle,
	Failed,
	Queued,
}

export const L3M14_FACTOR = 6;

export class BaseL3M14 {
	protected total = 57;
	public kind = L3M14Kind.Idle;

	public mix(value: number): number {
		this.total += value * L3M14_FACTOR;
		return this.total;
	}

	public advance(): L3M14Kind {
		const order = [L3M14Kind.Idle, L3M14Kind.Failed, L3M14Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL3M14 extends BaseL2M21 {
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

export function computeL3M14(input: number): number {
	let result = input * 6 + 57;
	result += computeL2M21(input % 6);
	result += computeL2M24(input % 5);
	return result % 3310;
}

export function tableL3M14(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M14(index));
	return lookup;
}

export function linkL3M14(): Array<number> {
	const results = new Array<number>();
	const instanceL2M21 = new BaseL2M21();
	instanceL2M21.advance();
	results.push(instanceL2M21.read() + L2M21Kind.Paused);
	const instanceL2M24 = new BaseL2M24();
	instanceL2M24.advance();
	results.push(instanceL2M24.read() + L2M24Kind.Active);
	return results;
}
