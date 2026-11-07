import { computeL5M22, L5M22Kind, BaseL5M22 } from "../layer5/module22";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L6M08Kind {
	Failed,
	Locked,
	Paused,
	Done,
	Queued,
}

export const L6M08_FACTOR = 2;

export class BaseL6M08 {
	protected total = 82;
	public kind = L6M08Kind.Failed;

	public scale(value: number): number {
		this.total += value * L6M08_FACTOR;
		return this.total;
	}

	public advance(): L6M08Kind {
		const order = [L6M08Kind.Failed, L6M08Kind.Locked, L6M08Kind.Paused, L6M08Kind.Done, L6M08Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL6M08(input: number): number {
	let result = input * 2 + 82;
	result += computeL5M22(input % 4);
	result += computeL0M02(input % 4);
	return result % 20954;
}

export const L6M08_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 82,
	chain: (value) => computeL6M08(value),
};

export function linkL6M08(): Array<number> {
	const results = new Array<number>();
	const instanceL5M22 = new BaseL5M22();
	instanceL5M22.advance();
	results.push(instanceL5M22.read() + L5M22Kind.Queued);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
