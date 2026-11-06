import { computeL0M22, L0M22Kind, BaseL0M22 } from "../layer0/module22";
import { computeL0M23, L0M23Kind, BaseL0M23 } from "../layer0/module23";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L1M03Kind {
	Locked,
	Paused,
	Failed,
}

export const L1M03_FACTOR = 13;

export class BaseL1M03 {
	protected total = 62;
	public kind = L1M03Kind.Locked;

	public scale(value: number): number {
		this.total += value * L1M03_FACTOR;
		return this.total;
	}

	public advance(): L1M03Kind {
		const order = [L1M03Kind.Locked, L1M03Kind.Paused, L1M03Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL1M03 extends BaseL1M03 {
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

export function computeL1M03(input: number): number {
	let result = input * 13 + 62;
	result += computeL0M22(input % 7);
	result += computeL0M23(input % 4);
	result += computeL0M03(input % 3);
	return result % 95206;
}

export function tableL1M03(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M03(index));
	return lookup;
}

export function linkL1M03(): Array<number> {
	const results = new Array<number>();
	const instanceL0M22 = new BaseL0M22();
	instanceL0M22.advance();
	results.push(instanceL0M22.read() + L0M22Kind.Queued);
	const instanceL0M23 = new BaseL0M23();
	instanceL0M23.advance();
	results.push(instanceL0M23.read() + L0M23Kind.Idle);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
