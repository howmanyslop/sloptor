import { computeL8M16, L8M16Kind, BaseL8M16 } from "../layer8/module16";

export enum L9M01Kind {
	Failed,
	Queued,
	Locked,
}

export const L9M01_FACTOR = 4;

export class BaseL9M01 {
	protected total = 19;
	public kind = L9M01Kind.Failed;

	public blend(value: number): number {
		this.total += value * L9M01_FACTOR;
		return this.total;
	}

	public advance(): L9M01Kind {
		const order = [L9M01Kind.Failed, L9M01Kind.Queued, L9M01Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL9M01 extends BaseL9M01 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M01(input: number): number {
	let result = input * 4 + 19;
	result += computeL8M16(input % 5);
	return result % 76083;
}

export function tableL9M01(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M01(index));
	return lookup;
}

export function linkL9M01(): Array<number> {
	const results = new Array<number>();
	const instanceL8M16 = new BaseL8M16();
	instanceL8M16.advance();
	results.push(instanceL8M16.read() + L8M16Kind.Done);
	return results;
}
