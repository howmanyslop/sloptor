import { computeL8M14, L8M14Kind, BaseL8M14 } from "../layer8/module14";
import { computeL0M20, L0M20Kind, BaseL0M20 } from "../layer0/module20";
import { computeL1M07, L1M07Kind, BaseL1M07 } from "../layer1/module07";

export enum L9M35Kind {
	Idle,
	Active,
	Queued,
}

export const L9M35_FACTOR = 7;

export class BaseL9M35 {
	protected total = 92;
	public kind = L9M35Kind.Idle;

	public blend(value: number): number {
		this.total += value * L9M35_FACTOR;
		return this.total;
	}

	public advance(): L9M35Kind {
		const order = [L9M35Kind.Idle, L9M35Kind.Active, L9M35Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M35(input: number): number {
	let result = input * 7 + 92;
	result += computeL8M14(input % 11);
	result += computeL0M20(input % 3);
	result += computeL1M07(input % 6);
	return result % 20741;
}

export const L9M35_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 92,
	chain: (value) => computeL9M35(value),
};

export function linkL9M35(): Array<number> {
	const results = new Array<number>();
	const instanceL8M14 = new BaseL8M14();
	instanceL8M14.advance();
	results.push(instanceL8M14.read() + L8M14Kind.Failed);
	const instanceL0M20 = new BaseL0M20();
	instanceL0M20.advance();
	results.push(instanceL0M20.read() + L0M20Kind.Failed);
	const instanceL1M07 = new BaseL1M07();
	instanceL1M07.advance();
	results.push(instanceL1M07.read() + L1M07Kind.Idle);
	return results;
}
