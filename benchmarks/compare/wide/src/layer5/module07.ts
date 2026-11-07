import { computeL4M14, L4M14Kind, BaseL4M14 } from "../layer4/module14";

export enum L5M07Kind {
	Idle,
	Active,
	Paused,
	Done,
	Queued,
}

export const L5M07_FACTOR = 14;

export class BaseL5M07 {
	protected total = 5;
	public kind = L5M07Kind.Idle;

	public fold(value: number): number {
		this.total += value * L5M07_FACTOR;
		return this.total;
	}

	public advance(): L5M07Kind {
		const order = [L5M07Kind.Idle, L5M07Kind.Active, L5M07Kind.Paused, L5M07Kind.Done, L5M07Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M07(input: number): number {
	let result = input * 14 + 5;
	result += computeL4M14(input % 9);
	return result % 45031;
}

export const L5M07_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 5,
	chain: (value) => computeL5M07(value),
};

export function linkL5M07(): Array<number> {
	const results = new Array<number>();
	const instanceL4M14 = new BaseL4M14();
	instanceL4M14.advance();
	results.push(instanceL4M14.read() + L4M14Kind.Idle);
	return results;
}
