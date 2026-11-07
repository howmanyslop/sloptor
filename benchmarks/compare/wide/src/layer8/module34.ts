import { computeL7M06, L7M06Kind, BaseL7M06 } from "../layer7/module06";

export enum L8M34Kind {
	Idle,
	Active,
	Queued,
}

export const L8M34_FACTOR = 3;

export class BaseL8M34 {
	protected total = 34;
	public kind = L8M34Kind.Idle;

	public blend(value: number): number {
		this.total += value * L8M34_FACTOR;
		return this.total;
	}

	public advance(): L8M34Kind {
		const order = [L8M34Kind.Idle, L8M34Kind.Active, L8M34Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL8M34(input: number): number {
	let result = input * 3 + 34;
	result += computeL7M06(input % 10);
	return result % 35087;
}

export const L8M34_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 34,
	chain: (value) => computeL8M34(value),
};

export function linkL8M34(): Array<number> {
	const results = new Array<number>();
	const instanceL7M06 = new BaseL7M06();
	instanceL7M06.advance();
	results.push(instanceL7M06.read() + L7M06Kind.Locked);
	return results;
}
