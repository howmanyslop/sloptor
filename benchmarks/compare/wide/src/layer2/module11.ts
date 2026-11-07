import { computeL1M39, L1M39Kind, BaseL1M39 } from "../layer1/module39";
import { computeL0M31, L0M31Kind, BaseL0M31 } from "../layer0/module31";
import { computeL0M25, L0M25Kind, BaseL0M25 } from "../layer0/module25";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L2M11Kind {
	Paused,
	Queued,
	Idle,
	Locked,
	Done,
}

export const L2M11_FACTOR = 19;

export class BaseL2M11 {
	protected total = 67;
	public kind = L2M11Kind.Paused;

	public scale(value: number): number {
		this.total += value * L2M11_FACTOR;
		return this.total;
	}

	public advance(): L2M11Kind {
		const order = [L2M11Kind.Paused, L2M11Kind.Queued, L2M11Kind.Idle, L2M11Kind.Locked, L2M11Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL2M11 extends BaseL2M11 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M11(input: number): number {
	let result = input * 19 + 67;
	result += computeL1M39(input % 4);
	result += computeL0M31(input % 3);
	result += computeL0M25(input % 9);
	result += computeL0M00(input % 7);
	return result % 50639;
}

export const L2M11_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 67,
	chain: (value) => computeL2M11(value),
};

export function linkL2M11(): Array<number> {
	const results = new Array<number>();
	const instanceL1M39 = new BaseL1M39();
	instanceL1M39.advance();
	results.push(instanceL1M39.read() + L1M39Kind.Done);
	const instanceL0M31 = new BaseL0M31();
	instanceL0M31.advance();
	results.push(instanceL0M31.read() + L0M31Kind.Failed);
	const instanceL0M25 = new BaseL0M25();
	instanceL0M25.advance();
	results.push(instanceL0M25.read() + L0M25Kind.Done);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
