import { computeL2M23, L2M23Kind, BaseL2M23 } from "../layer2/module23";

export enum L3M20Kind {
	Paused,
	Active,
	Failed,
	Done,
	Locked,
}

export const L3M20_FACTOR = 18;

export class BaseL3M20 {
	protected total = 82;
	public kind = L3M20Kind.Paused;

	public blend(value: number): number {
		this.total += value * L3M20_FACTOR;
		return this.total;
	}

	public advance(): L3M20Kind {
		const order = [L3M20Kind.Paused, L3M20Kind.Active, L3M20Kind.Failed, L3M20Kind.Done, L3M20Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL3M20(input: number): number {
	let result = input * 18 + 82;
	result += computeL2M23(input % 7);
	return result % 50313;
}

export const L3M20_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 82,
	chain: (value) => computeL3M20(value),
};

export function linkL3M20(): Array<number> {
	const results = new Array<number>();
	const instanceL2M23 = new BaseL2M23();
	instanceL2M23.advance();
	results.push(instanceL2M23.read() + L2M23Kind.Queued);
	return results;
}
