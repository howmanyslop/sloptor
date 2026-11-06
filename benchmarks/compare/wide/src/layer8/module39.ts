import { computeL7M01, L7M01Kind, BaseL7M01 } from "../layer7/module01";
import { computeL6M13, L6M13Kind, BaseL6M13 } from "../layer6/module13";
import { computeL2M26, L2M26Kind, BaseL2M26 } from "../layer2/module26";

export enum L8M39Kind {
	Done,
	Paused,
	Active,
	Failed,
}

export const L8M39_FACTOR = 18;

export class BaseL8M39 {
	protected total = 6;
	public kind = L8M39Kind.Done;

	public shift(value: number): number {
		this.total += value * L8M39_FACTOR;
		return this.total;
	}

	public advance(): L8M39Kind {
		const order = [L8M39Kind.Done, L8M39Kind.Paused, L8M39Kind.Active, L8M39Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL8M39 extends BaseL8M39 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 14) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M39(input: number): number {
	let result = input * 18 + 6;
	result += computeL7M01(input % 11);
	result += computeL6M13(input % 9);
	result += computeL2M26(input % 3);
	return result % 39711;
}

export function tableL8M39(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M39(index));
	return lookup;
}

export function linkL8M39(): Array<number> {
	const results = new Array<number>();
	const instanceL7M01 = new BaseL7M01();
	instanceL7M01.advance();
	results.push(instanceL7M01.read() + L7M01Kind.Failed);
	const instanceL6M13 = new BaseL6M13();
	instanceL6M13.advance();
	results.push(instanceL6M13.read() + L6M13Kind.Failed);
	const instanceL2M26 = new BaseL2M26();
	instanceL2M26.advance();
	results.push(instanceL2M26.read() + L2M26Kind.Idle);
	return results;
}
