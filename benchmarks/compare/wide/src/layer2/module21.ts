import { computeL1M29, L1M29Kind, BaseL1M29 } from "../layer1/module29";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L2M21Kind {
	Paused,
	Idle,
	Failed,
}

export const L2M21_FACTOR = 12;

export class BaseL2M21 {
	protected total = 23;
	public kind = L2M21Kind.Paused;

	public weigh(value: number): number {
		this.total += value * L2M21_FACTOR;
		return this.total;
	}

	public advance(): L2M21Kind {
		const order = [L2M21Kind.Paused, L2M21Kind.Idle, L2M21Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL2M21(input: number): number {
	let result = input * 12 + 23;
	result += computeL1M29(input % 4);
	result += computeL0M02(input % 4);
	return result % 60986;
}

export function describeL2M21(kind: L2M21Kind): string {
	switch (kind) {
		case L2M21Kind.Paused:
			return "paused";
		case L2M21Kind.Idle:
			return "idle";
		case L2M21Kind.Failed:
			return "failed";
	}
}

export function linkL2M21(): Array<number> {
	const results = new Array<number>();
	const instanceL1M29 = new BaseL1M29();
	instanceL1M29.advance();
	results.push(instanceL1M29.read() + L1M29Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
