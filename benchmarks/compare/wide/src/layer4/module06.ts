import { computeL3M06, L3M06Kind, BaseL3M06 } from "../layer3/module06";
import { computeL1M08, L1M08Kind, BaseL1M08 } from "../layer1/module08";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L4M06Kind {
	Paused,
	Locked,
	Idle,
}

export const L4M06_FACTOR = 16;

export class BaseL4M06 {
	protected total = 17;
	public kind = L4M06Kind.Paused;

	public scale(value: number): number {
		this.total += value * L4M06_FACTOR;
		return this.total;
	}

	public advance(): L4M06Kind {
		const order = [L4M06Kind.Paused, L4M06Kind.Locked, L4M06Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL4M06(input: number): number {
	let result = input * 16 + 17;
	result += computeL3M06(input % 9);
	result += computeL1M08(input % 7);
	result += computeL0M00(input % 5);
	return result % 87752;
}

export const L4M06_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 17,
	chain: (value) => computeL4M06(value),
};

export function linkL4M06(): Array<number> {
	const results = new Array<number>();
	const instanceL3M06 = new BaseL3M06();
	instanceL3M06.advance();
	results.push(instanceL3M06.read() + L3M06Kind.Done);
	const instanceL1M08 = new BaseL1M08();
	instanceL1M08.advance();
	results.push(instanceL1M08.read() + L1M08Kind.Active);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
