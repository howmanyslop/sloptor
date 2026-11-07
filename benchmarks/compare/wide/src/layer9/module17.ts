import { computeL8M08, L8M08Kind, BaseL8M08 } from "../layer8/module08";

export enum L9M17Kind {
	Failed,
	Idle,
	Active,
	Paused,
}

export const L9M17_FACTOR = 14;

export class BaseL9M17 {
	protected total = 5;
	public kind = L9M17Kind.Failed;

	public shift(value: number): number {
		this.total += value * L9M17_FACTOR;
		return this.total;
	}

	public advance(): L9M17Kind {
		const order = [L9M17Kind.Failed, L9M17Kind.Idle, L9M17Kind.Active, L9M17Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M17(input: number): number {
	let result = input * 14 + 5;
	result += computeL8M08(input % 4);
	return result % 14909;
}

export function tableL9M17(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M17(index));
	return lookup;
}

export function linkL9M17(): Array<number> {
	const results = new Array<number>();
	const instanceL8M08 = new BaseL8M08();
	instanceL8M08.advance();
	results.push(instanceL8M08.read() + L8M08Kind.Locked);
	return results;
}
