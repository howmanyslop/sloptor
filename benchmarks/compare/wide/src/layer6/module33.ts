import { computeL5M37, L5M37Kind, BaseL5M37 } from "../layer5/module37";

export enum L6M33Kind {
	Idle,
	Active,
	Paused,
}

export const L6M33_FACTOR = 4;

export class BaseL6M33 {
	protected total = 69;
	public kind = L6M33Kind.Idle;

	public warp(value: number): number {
		this.total += value * L6M33_FACTOR;
		return this.total;
	}

	public advance(): L6M33Kind {
		const order = [L6M33Kind.Idle, L6M33Kind.Active, L6M33Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL6M33 extends BaseL5M37 {
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

export function computeL6M33(input: number): number {
	let result = input * 4 + 69;
	result += computeL5M37(input % 4);
	return result % 32833;
}

export function tableL6M33(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M33(index));
	return lookup;
}

export function linkL6M33(): Array<number> {
	const results = new Array<number>();
	const instanceL5M37 = new BaseL5M37();
	instanceL5M37.advance();
	results.push(instanceL5M37.read() + L5M37Kind.Paused);
	return results;
}
