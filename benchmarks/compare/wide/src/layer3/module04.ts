import { computeL2M31, L2M31Kind, BaseL2M31 } from "../layer2/module31";
import { computeL1M36, L1M36Kind, BaseL1M36 } from "../layer1/module36";

export enum L3M04Kind {
	Paused,
	Locked,
	Done,
}

export const L3M04_FACTOR = 10;

export class BaseL3M04 {
	protected total = 80;
	public kind = L3M04Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L3M04_FACTOR;
		return this.total;
	}

	public advance(): L3M04Kind {
		const order = [L3M04Kind.Paused, L3M04Kind.Locked, L3M04Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M04(input: number): number {
	let result = input * 10 + 80;
	result += computeL2M31(input % 8);
	result += computeL1M36(input % 8);
	return result % 78980;
}

export const L3M04_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 80,
	chain: (value) => computeL3M04(value),
};

export function linkL3M04(): Array<number> {
	const results = new Array<number>();
	const instanceL2M31 = new BaseL2M31();
	instanceL2M31.advance();
	results.push(instanceL2M31.read() + L2M31Kind.Idle);
	const instanceL1M36 = new BaseL1M36();
	instanceL1M36.advance();
	results.push(instanceL1M36.read() + L1M36Kind.Idle);
	return results;
}
