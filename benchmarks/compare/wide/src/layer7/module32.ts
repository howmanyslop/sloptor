import { computeL6M31, L6M31Kind, BaseL6M31 } from "../layer6/module31";
import { computeL1M32, L1M32Kind, BaseL1M32 } from "../layer1/module32";

export enum L7M32Kind {
	Idle,
	Active,
	Paused,
}

export const L7M32_FACTOR = 15;

export class BaseL7M32 {
	protected total = 36;
	public kind = L7M32Kind.Idle;

	public tally(value: number): number {
		this.total += value * L7M32_FACTOR;
		return this.total;
	}

	public advance(): L7M32Kind {
		const order = [L7M32Kind.Idle, L7M32Kind.Active, L7M32Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M32(input: number): number {
	let result = input * 15 + 36;
	result += computeL6M31(input % 4);
	result += computeL1M32(input % 9);
	return result % 12810;
}

export const L7M32_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 36,
	chain: (value) => computeL7M32(value),
};

export function linkL7M32(): Array<number> {
	const results = new Array<number>();
	const instanceL6M31 = new BaseL6M31();
	instanceL6M31.advance();
	results.push(instanceL6M31.read() + L6M31Kind.Locked);
	const instanceL1M32 = new BaseL1M32();
	instanceL1M32.advance();
	results.push(instanceL1M32.read() + L1M32Kind.Idle);
	return results;
}
