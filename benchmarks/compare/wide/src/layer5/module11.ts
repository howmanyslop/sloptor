import { computeL4M02, L4M02Kind, BaseL4M02 } from "../layer4/module02";
import { computeL0M33, L0M33Kind, BaseL0M33 } from "../layer0/module33";
import { computeL1M24, L1M24Kind, BaseL1M24 } from "../layer1/module24";

export enum L5M11Kind {
	Active,
	Queued,
	Paused,
}

export const L5M11_FACTOR = 8;

export class BaseL5M11 {
	protected total = 32;
	public kind = L5M11Kind.Active;

	public fold(value: number): number {
		this.total += value * L5M11_FACTOR;
		return this.total;
	}

	public advance(): L5M11Kind {
		const order = [L5M11Kind.Active, L5M11Kind.Queued, L5M11Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL5M11(input: number): number {
	let result = input * 8 + 32;
	result += computeL4M02(input % 5);
	result += computeL0M33(input % 8);
	result += computeL1M24(input % 7);
	return result % 74220;
}

export function describeL5M11(kind: L5M11Kind): string {
	switch (kind) {
		case L5M11Kind.Active:
			return "active";
		case L5M11Kind.Queued:
			return "queued";
		case L5M11Kind.Paused:
			return "paused";
	}
}

export function linkL5M11(): Array<number> {
	const results = new Array<number>();
	const instanceL4M02 = new BaseL4M02();
	instanceL4M02.advance();
	results.push(instanceL4M02.read() + L4M02Kind.Queued);
	const instanceL0M33 = new BaseL0M33();
	instanceL0M33.advance();
	results.push(instanceL0M33.read() + L0M33Kind.Idle);
	const instanceL1M24 = new BaseL1M24();
	instanceL1M24.advance();
	results.push(instanceL1M24.read() + L1M24Kind.Active);
	return results;
}
