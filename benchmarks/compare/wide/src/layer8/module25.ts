import { computeL7M23, L7M23Kind, BaseL7M23 } from "../layer7/module23";
import { computeL3M00, L3M00Kind, BaseL3M00 } from "../layer3/module00";

export enum L8M25Kind {
	Done,
	Idle,
	Queued,
	Active,
}

export const L8M25_FACTOR = 16;

export class BaseL8M25 {
	protected total = 43;
	public kind = L8M25Kind.Done;

	public weigh(value: number): number {
		this.total += value * L8M25_FACTOR;
		return this.total;
	}

	public advance(): L8M25Kind {
		const order = [L8M25Kind.Done, L8M25Kind.Idle, L8M25Kind.Queued, L8M25Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL8M25 extends BaseL8M25 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 7) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M25(input: number): number {
	let result = input * 16 + 43;
	result += computeL7M23(input % 7);
	result += computeL3M00(input % 6);
	return result % 82461;
}

export function tableL8M25(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M25(index));
	return lookup;
}

export function linkL8M25(): Array<number> {
	const results = new Array<number>();
	const instanceL7M23 = new BaseL7M23();
	instanceL7M23.advance();
	results.push(instanceL7M23.read() + L7M23Kind.Paused);
	const instanceL3M00 = new BaseL3M00();
	instanceL3M00.advance();
	results.push(instanceL3M00.read() + L3M00Kind.Idle);
	return results;
}
