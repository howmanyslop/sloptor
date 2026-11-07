import { computeL1M34, L1M34Kind, BaseL1M34 } from "../layer1/module34";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L2M16Kind {
	Paused,
	Done,
	Failed,
}

export const L2M16_FACTOR = 9;

export class BaseL2M16 {
	protected total = 53;
	public kind = L2M16Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L2M16_FACTOR;
		return this.total;
	}

	public advance(): L2M16Kind {
		const order = [L2M16Kind.Paused, L2M16Kind.Done, L2M16Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M16(input: number): number {
	let result = input * 9 + 53;
	result += computeL1M34(input % 3);
	result += computeL0M00(input % 9);
	return result % 78075;
}

export const L2M16_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 53,
	chain: (value) => computeL2M16(value),
};

export function linkL2M16(): Array<number> {
	const results = new Array<number>();
	const instanceL1M34 = new BaseL1M34();
	instanceL1M34.advance();
	results.push(instanceL1M34.read() + L1M34Kind.Paused);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
