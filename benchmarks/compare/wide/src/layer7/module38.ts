import { computeL6M33, L6M33Kind, BaseL6M33 } from "../layer6/module33";

export enum L7M38Kind {
	Locked,
	Queued,
	Paused,
	Done,
	Failed,
}

export const L7M38_FACTOR = 5;

export class BaseL7M38 {
	protected total = 34;
	public kind = L7M38Kind.Locked;

	public weigh(value: number): number {
		this.total += value * L7M38_FACTOR;
		return this.total;
	}

	public advance(): L7M38Kind {
		const order = [L7M38Kind.Locked, L7M38Kind.Queued, L7M38Kind.Paused, L7M38Kind.Done, L7M38Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M38(input: number): number {
	let result = input * 5 + 34;
	result += computeL6M33(input % 8);
	return result % 94851;
}

export const L7M38_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 34,
	chain: (value) => computeL7M38(value),
};

export function linkL7M38(): Array<number> {
	const results = new Array<number>();
	const instanceL6M33 = new BaseL6M33();
	instanceL6M33.advance();
	results.push(instanceL6M33.read() + L6M33Kind.Idle);
	return results;
}
