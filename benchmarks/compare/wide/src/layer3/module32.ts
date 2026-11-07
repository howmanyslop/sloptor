import { computeL2M27, L2M27Kind, BaseL2M27 } from "../layer2/module27";
import { computeL1M31, L1M31Kind, BaseL1M31 } from "../layer1/module31";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L3M32Kind {
	Queued,
	Paused,
	Locked,
	Done,
}

export const L3M32_FACTOR = 5;

export class BaseL3M32 {
	protected total = 25;
	public kind = L3M32Kind.Queued;

	public fold(value: number): number {
		this.total += value * L3M32_FACTOR;
		return this.total;
	}

	public advance(): L3M32Kind {
		const order = [L3M32Kind.Queued, L3M32Kind.Paused, L3M32Kind.Locked, L3M32Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL3M32 extends BaseL3M32 {
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

export function computeL3M32(input: number): number {
	let result = input * 5 + 25;
	result += computeL2M27(input % 3);
	result += computeL1M31(input % 6);
	result += computeL0M00(input % 7);
	return result % 34662;
}

export const L3M32_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 25,
	chain: (value) => computeL3M32(value),
};

export function linkL3M32(): Array<number> {
	const results = new Array<number>();
	const instanceL2M27 = new BaseL2M27();
	instanceL2M27.advance();
	results.push(instanceL2M27.read() + L2M27Kind.Failed);
	const instanceL1M31 = new BaseL1M31();
	instanceL1M31.advance();
	results.push(instanceL1M31.read() + L1M31Kind.Done);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
