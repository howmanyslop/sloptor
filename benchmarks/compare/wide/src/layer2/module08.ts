import { computeL1M18, L1M18Kind, BaseL1M18 } from "../layer1/module18";

export enum L2M08Kind {
	Failed,
	Done,
	Queued,
	Paused,
	Active,
}

export const L2M08_FACTOR = 15;

export class BaseL2M08 {
	protected total = 47;
	public kind = L2M08Kind.Failed;

	public shift(value: number): number {
		this.total += value * L2M08_FACTOR;
		return this.total;
	}

	public advance(): L2M08Kind {
		const order = [L2M08Kind.Failed, L2M08Kind.Done, L2M08Kind.Queued, L2M08Kind.Paused, L2M08Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M08(input: number): number {
	let result = input * 15 + 47;
	result += computeL1M18(input % 11);
	return result % 88104;
}

export const L2M08_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 47,
	chain: (value) => computeL2M08(value),
};

export function linkL2M08(): Array<number> {
	const results = new Array<number>();
	const instanceL1M18 = new BaseL1M18();
	instanceL1M18.advance();
	results.push(instanceL1M18.read() + L1M18Kind.Idle);
	return results;
}
