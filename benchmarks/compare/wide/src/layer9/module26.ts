import { computeL8M31, L8M31Kind, BaseL8M31 } from "../layer8/module31";
import { computeL0M17, L0M17Kind, BaseL0M17 } from "../layer0/module17";
import { computeL7M00, L7M00Kind, BaseL7M00 } from "../layer7/module00";

export enum L9M26Kind {
	Active,
	Failed,
	Queued,
	Locked,
}

export const L9M26_FACTOR = 4;

export class BaseL9M26 {
	protected total = 97;
	public kind = L9M26Kind.Active;

	public scale(value: number): number {
		this.total += value * L9M26_FACTOR;
		return this.total;
	}

	public advance(): L9M26Kind {
		const order = [L9M26Kind.Active, L9M26Kind.Failed, L9M26Kind.Queued, L9M26Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M26(input: number): number {
	let result = input * 4 + 97;
	result += computeL8M31(input % 6);
	result += computeL0M17(input % 9);
	result += computeL7M00(input % 6);
	return result % 89023;
}

export const L9M26_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 97,
	chain: (value) => computeL9M26(value),
};

export function linkL9M26(): Array<number> {
	const results = new Array<number>();
	const instanceL8M31 = new BaseL8M31();
	instanceL8M31.advance();
	results.push(instanceL8M31.read() + L8M31Kind.Paused);
	const instanceL0M17 = new BaseL0M17();
	instanceL0M17.advance();
	results.push(instanceL0M17.read() + L0M17Kind.Paused);
	const instanceL7M00 = new BaseL7M00();
	instanceL7M00.advance();
	results.push(instanceL7M00.read() + L7M00Kind.Queued);
	return results;
}
