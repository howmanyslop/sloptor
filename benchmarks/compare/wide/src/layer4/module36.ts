import { computeL3M16, L3M16Kind, BaseL3M16 } from "../layer3/module16";
import { computeL2M01, L2M01Kind, BaseL2M01 } from "../layer2/module01";
import { computeL1M29, L1M29Kind, BaseL1M29 } from "../layer1/module29";

export enum L4M36Kind {
	Idle,
	Queued,
	Locked,
}

export const L4M36_FACTOR = 16;

export class BaseL4M36 {
	protected total = 58;
	public kind = L4M36Kind.Idle;

	public fold(value: number): number {
		this.total += value * L4M36_FACTOR;
		return this.total;
	}

	public advance(): L4M36Kind {
		const order = [L4M36Kind.Idle, L4M36Kind.Queued, L4M36Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL4M36 extends BaseL3M16 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M36(input: number): number {
	let result = input * 16 + 58;
	result += computeL3M16(input % 9);
	result += computeL2M01(input % 6);
	result += computeL1M29(input % 11);
	return result % 13812;
}

export const L4M36_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 58,
	chain: (value) => computeL4M36(value),
};

export function linkL4M36(): Array<number> {
	const results = new Array<number>();
	const instanceL3M16 = new BaseL3M16();
	instanceL3M16.advance();
	results.push(instanceL3M16.read() + L3M16Kind.Idle);
	const instanceL2M01 = new BaseL2M01();
	instanceL2M01.advance();
	results.push(instanceL2M01.read() + L2M01Kind.Failed);
	const instanceL1M29 = new BaseL1M29();
	instanceL1M29.advance();
	results.push(instanceL1M29.read() + L1M29Kind.Idle);
	return results;
}
