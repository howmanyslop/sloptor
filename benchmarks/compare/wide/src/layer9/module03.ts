import { computeL8M30, L8M30Kind, BaseL8M30 } from "../layer8/module30";

export enum L9M03Kind {
	Idle,
	Queued,
	Locked,
}

export const L9M03_FACTOR = 10;

export class BaseL9M03 {
	protected total = 88;
	public kind = L9M03Kind.Idle;

	public fold(value: number): number {
		this.total += value * L9M03_FACTOR;
		return this.total;
	}

	public advance(): L9M03Kind {
		const order = [L9M03Kind.Idle, L9M03Kind.Queued, L9M03Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M03(input: number): number {
	let result = input * 10 + 88;
	result += computeL8M30(input % 11);
	return result % 89279;
}

export const L9M03_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 88,
	chain: (value) => computeL9M03(value),
};

export function linkL9M03(): Array<number> {
	const results = new Array<number>();
	const instanceL8M30 = new BaseL8M30();
	instanceL8M30.advance();
	results.push(instanceL8M30.read() + L8M30Kind.Done);
	return results;
}
