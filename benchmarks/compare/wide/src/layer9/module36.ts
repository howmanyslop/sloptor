import { computeL8M21, L8M21Kind, BaseL8M21 } from "../layer8/module21";

export enum L9M36Kind {
	Idle,
	Failed,
	Locked,
}

export const L9M36_FACTOR = 17;

export class BaseL9M36 {
	protected total = 61;
	public kind = L9M36Kind.Idle;

	public blend(value: number): number {
		this.total += value * L9M36_FACTOR;
		return this.total;
	}

	public advance(): L9M36Kind {
		const order = [L9M36Kind.Idle, L9M36Kind.Failed, L9M36Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL9M36 extends BaseL8M21 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M36(input: number): number {
	let result = input * 17 + 61;
	result += computeL8M21(input % 3);
	return result % 98456;
}

export function tableL9M36(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M36(index));
	return lookup;
}

export function linkL9M36(): Array<number> {
	const results = new Array<number>();
	const instanceL8M21 = new BaseL8M21();
	instanceL8M21.advance();
	results.push(instanceL8M21.read() + L8M21Kind.Active);
	return results;
}
