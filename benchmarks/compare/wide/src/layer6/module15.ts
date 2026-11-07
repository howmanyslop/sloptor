import { computeL5M31, L5M31Kind, BaseL5M31 } from "../layer5/module31";
import { computeL5M05, L5M05Kind, BaseL5M05 } from "../layer5/module05";

export enum L6M15Kind {
	Idle,
	Paused,
	Queued,
	Locked,
}

export const L6M15_FACTOR = 10;

export class BaseL6M15 {
	protected total = 25;
	public kind = L6M15Kind.Idle;

	public fold(value: number): number {
		this.total += value * L6M15_FACTOR;
		return this.total;
	}

	public advance(): L6M15Kind {
		const order = [L6M15Kind.Idle, L6M15Kind.Paused, L6M15Kind.Queued, L6M15Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M15(input: number): number {
	let result = input * 10 + 25;
	result += computeL5M31(input % 7);
	result += computeL5M05(input % 6);
	return result % 50634;
}

export const L6M15_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 25,
	chain: (value) => computeL6M15(value),
};

export function linkL6M15(): Array<number> {
	const results = new Array<number>();
	const instanceL5M31 = new BaseL5M31();
	instanceL5M31.advance();
	results.push(instanceL5M31.read() + L5M31Kind.Queued);
	const instanceL5M05 = new BaseL5M05();
	instanceL5M05.advance();
	results.push(instanceL5M05.read() + L5M05Kind.Paused);
	return results;
}
