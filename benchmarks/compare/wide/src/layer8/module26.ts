import { computeL7M30, L7M30Kind, BaseL7M30 } from "../layer7/module30";
import { computeL0M09, L0M09Kind, BaseL0M09 } from "../layer0/module09";
import { computeL6M27, L6M27Kind, BaseL6M27 } from "../layer6/module27";

export enum L8M26Kind {
	Idle,
	Locked,
	Queued,
}

export const L8M26_FACTOR = 10;

export class BaseL8M26 {
	protected total = 40;
	public kind = L8M26Kind.Idle;

	public mix(value: number): number {
		this.total += value * L8M26_FACTOR;
		return this.total;
	}

	public advance(): L8M26Kind {
		const order = [L8M26Kind.Idle, L8M26Kind.Locked, L8M26Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL8M26 extends BaseL7M30 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M26(input: number): number {
	let result = input * 10 + 40;
	result += computeL7M30(input % 10);
	result += computeL0M09(input % 11);
	result += computeL6M27(input % 11);
	return result % 93326;
}

export function tableL8M26(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M26(index));
	return lookup;
}

export function linkL8M26(): Array<number> {
	const results = new Array<number>();
	const instanceL7M30 = new BaseL7M30();
	instanceL7M30.advance();
	results.push(instanceL7M30.read() + L7M30Kind.Idle);
	const instanceL0M09 = new BaseL0M09();
	instanceL0M09.advance();
	results.push(instanceL0M09.read() + L0M09Kind.Failed);
	const instanceL6M27 = new BaseL6M27();
	instanceL6M27.advance();
	results.push(instanceL6M27.read() + L6M27Kind.Queued);
	return results;
}
