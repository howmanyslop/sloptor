import { computeL4M01, L4M01Kind, BaseL4M01 } from "../layer4/module01";
import { computeL0M18, L0M18Kind, BaseL0M18 } from "../layer0/module18";
import { computeL1M09, L1M09Kind, BaseL1M09 } from "../layer1/module09";

export enum L5M28Kind {
	Active,
	Paused,
	Done,
}

export const L5M28_FACTOR = 10;

export class BaseL5M28 {
	protected total = 72;
	public kind = L5M28Kind.Active;

	public weigh(value: number): number {
		this.total += value * L5M28_FACTOR;
		return this.total;
	}

	public advance(): L5M28Kind {
		const order = [L5M28Kind.Active, L5M28Kind.Paused, L5M28Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M28(input: number): number {
	let result = input * 10 + 72;
	result += computeL4M01(input % 7);
	result += computeL0M18(input % 7);
	result += computeL1M09(input % 7);
	return result % 36718;
}

export function describeL5M28(kind: L5M28Kind): string {
	switch (kind) {
		case L5M28Kind.Active:
			return "active";
		case L5M28Kind.Paused:
			return "paused";
		case L5M28Kind.Done:
			return "done";
	}
}

export function linkL5M28(): Array<number> {
	const results = new Array<number>();
	const instanceL4M01 = new BaseL4M01();
	instanceL4M01.advance();
	results.push(instanceL4M01.read() + L4M01Kind.Locked);
	const instanceL0M18 = new BaseL0M18();
	instanceL0M18.advance();
	results.push(instanceL0M18.read() + L0M18Kind.Paused);
	const instanceL1M09 = new BaseL1M09();
	instanceL1M09.advance();
	results.push(instanceL1M09.read() + L1M09Kind.Idle);
	return results;
}
