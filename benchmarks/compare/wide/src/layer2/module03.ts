import { computeL1M23, L1M23Kind, BaseL1M23 } from "../layer1/module23";
import { computeL0M37, L0M37Kind, BaseL0M37 } from "../layer0/module37";
import { computeL0M34, L0M34Kind, BaseL0M34 } from "../layer0/module34";

export enum L2M03Kind {
	Active,
	Idle,
	Done,
	Failed,
}

export const L2M03_FACTOR = 5;

export class BaseL2M03 {
	protected total = 2;
	public kind = L2M03Kind.Active;

	public weigh(value: number): number {
		this.total += value * L2M03_FACTOR;
		return this.total;
	}

	public advance(): L2M03Kind {
		const order = [L2M03Kind.Active, L2M03Kind.Idle, L2M03Kind.Done, L2M03Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M03(input: number): number {
	let result = input * 5 + 2;
	result += computeL1M23(input % 3);
	result += computeL0M37(input % 9);
	result += computeL0M34(input % 8);
	return result % 11815;
}

export const L2M03_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 2,
	chain: (value) => computeL2M03(value),
};

export function linkL2M03(): Array<number> {
	const results = new Array<number>();
	const instanceL1M23 = new BaseL1M23();
	instanceL1M23.advance();
	results.push(instanceL1M23.read() + L1M23Kind.Idle);
	const instanceL0M37 = new BaseL0M37();
	instanceL0M37.advance();
	results.push(instanceL0M37.read() + L0M37Kind.Done);
	const instanceL0M34 = new BaseL0M34();
	instanceL0M34.advance();
	results.push(instanceL0M34.read() + L0M34Kind.Locked);
	return results;
}
