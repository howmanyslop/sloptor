import { computeL7M09, L7M09Kind, BaseL7M09 } from "../layer7/module09";
import { computeL3M14, L3M14Kind, BaseL3M14 } from "../layer3/module14";
import { computeL0M30, L0M30Kind, BaseL0M30 } from "../layer0/module30";

export enum L8M23Kind {
	Paused,
	Failed,
	Queued,
}

export const L8M23_FACTOR = 18;

export class BaseL8M23 {
	protected total = 80;
	public kind = L8M23Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L8M23_FACTOR;
		return this.total;
	}

	public advance(): L8M23Kind {
		const order = [L8M23Kind.Paused, L8M23Kind.Failed, L8M23Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL8M23 extends BaseL7M09 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 9) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M23(input: number): number {
	let result = input * 18 + 80;
	result += computeL7M09(input % 6);
	result += computeL3M14(input % 8);
	result += computeL0M30(input % 8);
	return result % 72001;
}

export function tableL8M23(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M23(index));
	return lookup;
}

export function linkL8M23(): Array<number> {
	const results = new Array<number>();
	const instanceL7M09 = new BaseL7M09();
	instanceL7M09.advance();
	results.push(instanceL7M09.read() + L7M09Kind.Paused);
	const instanceL3M14 = new BaseL3M14();
	instanceL3M14.advance();
	results.push(instanceL3M14.read() + L3M14Kind.Idle);
	const instanceL0M30 = new BaseL0M30();
	instanceL0M30.advance();
	results.push(instanceL0M30.read() + L0M30Kind.Done);
	return results;
}
