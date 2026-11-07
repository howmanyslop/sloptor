import { computeL3M37, L3M37Kind, BaseL3M37 } from "../layer3/module37";
import { computeL3M05, L3M05Kind, BaseL3M05 } from "../layer3/module05";
import { computeL1M35, L1M35Kind, BaseL1M35 } from "../layer1/module35";

export enum L4M39Kind {
	Idle,
	Active,
	Queued,
}

export const L4M39_FACTOR = 17;

export class BaseL4M39 {
	protected total = 7;
	public kind = L4M39Kind.Idle;

	public scale(value: number): number {
		this.total += value * L4M39_FACTOR;
		return this.total;
	}

	public advance(): L4M39Kind {
		const order = [L4M39Kind.Idle, L4M39Kind.Active, L4M39Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL4M39 extends BaseL3M37 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M39(input: number): number {
	let result = input * 17 + 7;
	result += computeL3M37(input % 9);
	result += computeL3M05(input % 3);
	result += computeL1M35(input % 4);
	return result % 3227;
}

export function tableL4M39(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M39(index));
	return lookup;
}

export function linkL4M39(): Array<number> {
	const results = new Array<number>();
	const instanceL3M37 = new BaseL3M37();
	instanceL3M37.advance();
	results.push(instanceL3M37.read() + L3M37Kind.Failed);
	const instanceL3M05 = new BaseL3M05();
	instanceL3M05.advance();
	results.push(instanceL3M05.read() + L3M05Kind.Failed);
	const instanceL1M35 = new BaseL1M35();
	instanceL1M35.advance();
	results.push(instanceL1M35.read() + L1M35Kind.Idle);
	return results;
}
