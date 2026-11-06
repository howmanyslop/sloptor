import { computeL6M04, L6M04Kind, BaseL6M04 } from "../layer6/module04";
import { computeL0M26, L0M26Kind, BaseL0M26 } from "../layer0/module26";

export enum L7M11Kind {
	Done,
	Queued,
	Active,
	Idle,
	Locked,
}

export const L7M11_FACTOR = 7;

export class BaseL7M11 {
	protected total = 61;
	public kind = L7M11Kind.Done;

	public fold(value: number): number {
		this.total += value * L7M11_FACTOR;
		return this.total;
	}

	public advance(): L7M11Kind {
		const order = [L7M11Kind.Done, L7M11Kind.Queued, L7M11Kind.Active, L7M11Kind.Idle, L7M11Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M11(input: number): number {
	let result = input * 7 + 61;
	result += computeL6M04(input % 4);
	result += computeL0M26(input % 6);
	return result % 94665;
}

export const L7M11_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 61,
	chain: (value) => computeL7M11(value),
};

export function linkL7M11(): Array<number> {
	const results = new Array<number>();
	const instanceL6M04 = new BaseL6M04();
	instanceL6M04.advance();
	results.push(instanceL6M04.read() + L6M04Kind.Locked);
	const instanceL0M26 = new BaseL0M26();
	instanceL0M26.advance();
	results.push(instanceL0M26.read() + L0M26Kind.Idle);
	return results;
}
