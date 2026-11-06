import { computeL3M33, L3M33Kind, BaseL3M33 } from "../layer3/module33";
import { computeL2M25, L2M25Kind, BaseL2M25 } from "../layer2/module25";
import { computeL3M32, L3M32Kind, BaseL3M32 } from "../layer3/module32";

export enum L4M27Kind {
	Done,
	Locked,
	Failed,
	Idle,
	Paused,
}

export const L4M27_FACTOR = 2;

export class BaseL4M27 {
	protected total = 24;
	public kind = L4M27Kind.Done;

	public shift(value: number): number {
		this.total += value * L4M27_FACTOR;
		return this.total;
	}

	public advance(): L4M27Kind {
		const order = [L4M27Kind.Done, L4M27Kind.Locked, L4M27Kind.Failed, L4M27Kind.Idle, L4M27Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL4M27 extends BaseL3M33 {
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

export function computeL4M27(input: number): number {
	let result = input * 2 + 24;
	result += computeL3M33(input % 10);
	result += computeL2M25(input % 8);
	result += computeL3M32(input % 9);
	return result % 2376;
}

export function tableL4M27(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M27(index));
	return lookup;
}

export function linkL4M27(): Array<number> {
	const results = new Array<number>();
	const instanceL3M33 = new BaseL3M33();
	instanceL3M33.advance();
	results.push(instanceL3M33.read() + L3M33Kind.Done);
	const instanceL2M25 = new BaseL2M25();
	instanceL2M25.advance();
	results.push(instanceL2M25.read() + L2M25Kind.Idle);
	const instanceL3M32 = new BaseL3M32();
	instanceL3M32.advance();
	results.push(instanceL3M32.read() + L3M32Kind.Queued);
	return results;
}
