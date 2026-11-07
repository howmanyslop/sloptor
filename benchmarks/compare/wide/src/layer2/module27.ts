import { computeL1M31, L1M31Kind, BaseL1M31 } from "../layer1/module31";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L2M27Kind {
	Failed,
	Idle,
	Active,
	Done,
	Paused,
}

export const L2M27_FACTOR = 2;

export class BaseL2M27 {
	protected total = 14;
	public kind = L2M27Kind.Failed;

	public scale(value: number): number {
		this.total += value * L2M27_FACTOR;
		return this.total;
	}

	public advance(): L2M27Kind {
		const order = [L2M27Kind.Failed, L2M27Kind.Idle, L2M27Kind.Active, L2M27Kind.Done, L2M27Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M27(input: number): number {
	let result = input * 2 + 14;
	result += computeL1M31(input % 11);
	result += computeL0M00(input % 4);
	return result % 5339;
}

export const L2M27_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 14,
	chain: (value) => computeL2M27(value),
};

export function linkL2M27(): Array<number> {
	const results = new Array<number>();
	const instanceL1M31 = new BaseL1M31();
	instanceL1M31.advance();
	results.push(instanceL1M31.read() + L1M31Kind.Done);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
