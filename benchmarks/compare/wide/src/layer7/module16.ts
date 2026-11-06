import { computeL6M39, L6M39Kind, BaseL6M39 } from "../layer6/module39";
import { computeL1M01, L1M01Kind, BaseL1M01 } from "../layer1/module01";
import { computeL2M06, L2M06Kind, BaseL2M06 } from "../layer2/module06";

export enum L7M16Kind {
	Locked,
	Failed,
	Idle,
}

export const L7M16_FACTOR = 2;

export class BaseL7M16 {
	protected total = 91;
	public kind = L7M16Kind.Locked;

	public scale(value: number): number {
		this.total += value * L7M16_FACTOR;
		return this.total;
	}

	public advance(): L7M16Kind {
		const order = [L7M16Kind.Locked, L7M16Kind.Failed, L7M16Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL7M16 extends BaseL7M16 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M16(input: number): number {
	let result = input * 2 + 91;
	result += computeL6M39(input % 11);
	result += computeL1M01(input % 8);
	result += computeL2M06(input % 5);
	return result % 73139;
}

export const L7M16_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 91,
	chain: (value) => computeL7M16(value),
};

export function linkL7M16(): Array<number> {
	const results = new Array<number>();
	const instanceL6M39 = new BaseL6M39();
	instanceL6M39.advance();
	results.push(instanceL6M39.read() + L6M39Kind.Done);
	const instanceL1M01 = new BaseL1M01();
	instanceL1M01.advance();
	results.push(instanceL1M01.read() + L1M01Kind.Locked);
	const instanceL2M06 = new BaseL2M06();
	instanceL2M06.advance();
	results.push(instanceL2M06.read() + L2M06Kind.Active);
	return results;
}
