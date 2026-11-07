import { computeL4M24, L4M24Kind, BaseL4M24 } from "../layer4/module24";

export enum L5M37Kind {
	Paused,
	Queued,
	Active,
	Locked,
}

export const L5M37_FACTOR = 18;

export class BaseL5M37 {
	protected total = 97;
	public kind = L5M37Kind.Paused;

	public blend(value: number): number {
		this.total += value * L5M37_FACTOR;
		return this.total;
	}

	public advance(): L5M37Kind {
		const order = [L5M37Kind.Paused, L5M37Kind.Queued, L5M37Kind.Active, L5M37Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL5M37 extends BaseL5M37 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL5M37(input: number): number {
	let result = input * 18 + 97;
	result += computeL4M24(input % 9);
	return result % 11212;
}

export function tableL5M37(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M37(index));
	return lookup;
}

export function linkL5M37(): Array<number> {
	const results = new Array<number>();
	const instanceL4M24 = new BaseL4M24();
	instanceL4M24.advance();
	results.push(instanceL4M24.read() + L4M24Kind.Queued);
	return results;
}
