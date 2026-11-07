import { computeL6M07, L6M07Kind, BaseL6M07 } from "../layer6/module07";
import { computeL1M39, L1M39Kind, BaseL1M39 } from "../layer1/module39";
import { computeL4M25, L4M25Kind, BaseL4M25 } from "../layer4/module25";

export enum L7M00Kind {
	Queued,
	Paused,
	Failed,
	Active,
}

export const L7M00_FACTOR = 17;

export class BaseL7M00 {
	protected total = 7;
	public kind = L7M00Kind.Queued;

	public mix(value: number): number {
		this.total += value * L7M00_FACTOR;
		return this.total;
	}

	public advance(): L7M00Kind {
		const order = [L7M00Kind.Queued, L7M00Kind.Paused, L7M00Kind.Failed, L7M00Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL7M00(input: number): number {
	let result = input * 17 + 7;
	result += computeL6M07(input % 11);
	result += computeL1M39(input % 4);
	result += computeL4M25(input % 3);
	return result % 65572;
}

export const L7M00_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 7,
	chain: (value) => computeL7M00(value),
};

export function linkL7M00(): Array<number> {
	const results = new Array<number>();
	const instanceL6M07 = new BaseL6M07();
	instanceL6M07.advance();
	results.push(instanceL6M07.read() + L6M07Kind.Idle);
	const instanceL1M39 = new BaseL1M39();
	instanceL1M39.advance();
	results.push(instanceL1M39.read() + L1M39Kind.Done);
	const instanceL4M25 = new BaseL4M25();
	instanceL4M25.advance();
	results.push(instanceL4M25.read() + L4M25Kind.Idle);
	return results;
}
