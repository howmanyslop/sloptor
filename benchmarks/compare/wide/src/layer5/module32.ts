import { computeL4M29, L4M29Kind, BaseL4M29 } from "../layer4/module29";
import { computeL2M23, L2M23Kind, BaseL2M23 } from "../layer2/module23";

export enum L5M32Kind {
	Queued,
	Locked,
	Active,
	Idle,
}

export const L5M32_FACTOR = 6;

export class BaseL5M32 {
	protected total = 21;
	public kind = L5M32Kind.Queued;

	public scale(value: number): number {
		this.total += value * L5M32_FACTOR;
		return this.total;
	}

	public advance(): L5M32Kind {
		const order = [L5M32Kind.Queued, L5M32Kind.Locked, L5M32Kind.Active, L5M32Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL5M32 extends BaseL5M32 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL5M32(input: number): number {
	let result = input * 6 + 21;
	result += computeL4M29(input % 4);
	result += computeL2M23(input % 3);
	return result % 94316;
}

export function tableL5M32(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M32(index));
	return lookup;
}

export function linkL5M32(): Array<number> {
	const results = new Array<number>();
	const instanceL4M29 = new BaseL4M29();
	instanceL4M29.advance();
	results.push(instanceL4M29.read() + L4M29Kind.Idle);
	const instanceL2M23 = new BaseL2M23();
	instanceL2M23.advance();
	results.push(instanceL2M23.read() + L2M23Kind.Queued);
	return results;
}
