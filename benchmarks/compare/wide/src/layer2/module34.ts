import { computeL1M00, L1M00Kind, BaseL1M00 } from "../layer1/module00";
import { computeL0M31, L0M31Kind, BaseL0M31 } from "../layer0/module31";
import { computeL0M32, L0M32Kind, BaseL0M32 } from "../layer0/module32";

export enum L2M34Kind {
	Failed,
	Done,
	Paused,
}

export const L2M34_FACTOR = 3;

export class BaseL2M34 {
	protected total = 6;
	public kind = L2M34Kind.Failed;

	public shift(value: number): number {
		this.total += value * L2M34_FACTOR;
		return this.total;
	}

	public advance(): L2M34Kind {
		const order = [L2M34Kind.Failed, L2M34Kind.Done, L2M34Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL2M34 extends BaseL1M00 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M34(input: number): number {
	let result = input * 3 + 6;
	result += computeL1M00(input % 4);
	result += computeL0M31(input % 10);
	result += computeL0M32(input % 7);
	return result % 24362;
}

export const L2M34_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 6,
	chain: (value) => computeL2M34(value),
};

export function linkL2M34(): Array<number> {
	const results = new Array<number>();
	const instanceL1M00 = new BaseL1M00();
	instanceL1M00.advance();
	results.push(instanceL1M00.read() + L1M00Kind.Paused);
	const instanceL0M31 = new BaseL0M31();
	instanceL0M31.advance();
	results.push(instanceL0M31.read() + L0M31Kind.Failed);
	const instanceL0M32 = new BaseL0M32();
	instanceL0M32.advance();
	results.push(instanceL0M32.read() + L0M32Kind.Paused);
	return results;
}
