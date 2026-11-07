import { computeL8M32, L8M32Kind, BaseL8M32 } from "../layer8/module32";
import { computeL5M20, L5M20Kind, BaseL5M20 } from "../layer5/module20";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L9M09Kind {
	Locked,
	Idle,
	Active,
	Paused,
}

export const L9M09_FACTOR = 6;

export class BaseL9M09 {
	protected total = 95;
	public kind = L9M09Kind.Locked;

	public warp(value: number): number {
		this.total += value * L9M09_FACTOR;
		return this.total;
	}

	public advance(): L9M09Kind {
		const order = [L9M09Kind.Locked, L9M09Kind.Idle, L9M09Kind.Active, L9M09Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL9M09 extends BaseL9M09 {
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

export function computeL9M09(input: number): number {
	let result = input * 6 + 95;
	result += computeL8M32(input % 9);
	result += computeL5M20(input % 7);
	result += computeL0M01(input % 6);
	return result % 49941;
}

export function tableL9M09(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M09(index));
	return lookup;
}

export function linkL9M09(): Array<number> {
	const results = new Array<number>();
	const instanceL8M32 = new BaseL8M32();
	instanceL8M32.advance();
	results.push(instanceL8M32.read() + L8M32Kind.Done);
	const instanceL5M20 = new BaseL5M20();
	instanceL5M20.advance();
	results.push(instanceL5M20.read() + L5M20Kind.Locked);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
