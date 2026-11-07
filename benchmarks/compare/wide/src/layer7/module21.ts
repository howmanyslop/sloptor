import { computeL6M34, L6M34Kind, BaseL6M34 } from "../layer6/module34";
import { computeL1M02, L1M02Kind, BaseL1M02 } from "../layer1/module02";
import { computeL3M13, L3M13Kind, BaseL3M13 } from "../layer3/module13";

export enum L7M21Kind {
	Locked,
	Idle,
	Active,
	Failed,
	Paused,
}

export const L7M21_FACTOR = 14;

export class BaseL7M21 {
	protected total = 84;
	public kind = L7M21Kind.Locked;

	public fold(value: number): number {
		this.total += value * L7M21_FACTOR;
		return this.total;
	}

	public advance(): L7M21Kind {
		const order = [L7M21Kind.Locked, L7M21Kind.Idle, L7M21Kind.Active, L7M21Kind.Failed, L7M21Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M21(input: number): number {
	let result = input * 14 + 84;
	result += computeL6M34(input % 6);
	result += computeL1M02(input % 7);
	result += computeL3M13(input % 8);
	return result % 66426;
}

export const L7M21_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 84,
	chain: (value) => computeL7M21(value),
};

export function linkL7M21(): Array<number> {
	const results = new Array<number>();
	const instanceL6M34 = new BaseL6M34();
	instanceL6M34.advance();
	results.push(instanceL6M34.read() + L6M34Kind.Idle);
	const instanceL1M02 = new BaseL1M02();
	instanceL1M02.advance();
	results.push(instanceL1M02.read() + L1M02Kind.Locked);
	const instanceL3M13 = new BaseL3M13();
	instanceL3M13.advance();
	results.push(instanceL3M13.read() + L3M13Kind.Queued);
	return results;
}
