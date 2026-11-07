import { computeL8M25, L8M25Kind, BaseL8M25 } from "../layer8/module25";
import { computeL2M03, L2M03Kind, BaseL2M03 } from "../layer2/module03";

export enum L9M08Kind {
	Idle,
	Locked,
	Queued,
	Done,
}

export const L9M08_FACTOR = 10;

export class BaseL9M08 {
	protected total = 9;
	public kind = L9M08Kind.Idle;

	public scale(value: number): number {
		this.total += value * L9M08_FACTOR;
		return this.total;
	}

	public advance(): L9M08Kind {
		const order = [L9M08Kind.Idle, L9M08Kind.Locked, L9M08Kind.Queued, L9M08Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL9M08 extends BaseL8M25 {
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

export function computeL9M08(input: number): number {
	let result = input * 10 + 9;
	result += computeL8M25(input % 4);
	result += computeL2M03(input % 8);
	return result % 98210;
}

export function tableL9M08(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M08(index));
	return lookup;
}

export function linkL9M08(): Array<number> {
	const results = new Array<number>();
	const instanceL8M25 = new BaseL8M25();
	instanceL8M25.advance();
	results.push(instanceL8M25.read() + L8M25Kind.Done);
	const instanceL2M03 = new BaseL2M03();
	instanceL2M03.advance();
	results.push(instanceL2M03.read() + L2M03Kind.Active);
	return results;
}
