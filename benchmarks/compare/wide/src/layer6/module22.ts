import { computeL5M00, L5M00Kind, BaseL5M00 } from "../layer5/module00";
import { computeL3M32, L3M32Kind, BaseL3M32 } from "../layer3/module32";

export enum L6M22Kind {
	Active,
	Locked,
	Done,
	Failed,
}

export const L6M22_FACTOR = 5;

export class BaseL6M22 {
	protected total = 84;
	public kind = L6M22Kind.Active;

	public tally(value: number): number {
		this.total += value * L6M22_FACTOR;
		return this.total;
	}

	public advance(): L6M22Kind {
		const order = [L6M22Kind.Active, L6M22Kind.Locked, L6M22Kind.Done, L6M22Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M22(input: number): number {
	let result = input * 5 + 84;
	result += computeL5M00(input % 6);
	result += computeL3M32(input % 7);
	return result % 2310;
}

export const L6M22_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 84,
	chain: (value) => computeL6M22(value),
};

export function linkL6M22(): Array<number> {
	const results = new Array<number>();
	const instanceL5M00 = new BaseL5M00();
	instanceL5M00.advance();
	results.push(instanceL5M00.read() + L5M00Kind.Queued);
	const instanceL3M32 = new BaseL3M32();
	instanceL3M32.advance();
	results.push(instanceL3M32.read() + L3M32Kind.Queued);
	return results;
}
