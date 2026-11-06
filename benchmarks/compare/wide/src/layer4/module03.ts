import { computeL3M25, L3M25Kind, BaseL3M25 } from "../layer3/module25";
import { computeL1M31, L1M31Kind, BaseL1M31 } from "../layer1/module31";

export enum L4M03Kind {
	Paused,
	Idle,
	Queued,
	Failed,
	Active,
}

export const L4M03_FACTOR = 11;

export class BaseL4M03 {
	protected total = 34;
	public kind = L4M03Kind.Paused;

	public shift(value: number): number {
		this.total += value * L4M03_FACTOR;
		return this.total;
	}

	public advance(): L4M03Kind {
		const order = [L4M03Kind.Paused, L4M03Kind.Idle, L4M03Kind.Queued, L4M03Kind.Failed, L4M03Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL4M03 extends BaseL4M03 {
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

export function computeL4M03(input: number): number {
	let result = input * 11 + 34;
	result += computeL3M25(input % 7);
	result += computeL1M31(input % 9);
	return result % 56312;
}

export function tableL4M03(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M03(index));
	return lookup;
}

export function linkL4M03(): Array<number> {
	const results = new Array<number>();
	const instanceL3M25 = new BaseL3M25();
	instanceL3M25.advance();
	results.push(instanceL3M25.read() + L3M25Kind.Failed);
	const instanceL1M31 = new BaseL1M31();
	instanceL1M31.advance();
	results.push(instanceL1M31.read() + L1M31Kind.Done);
	return results;
}
