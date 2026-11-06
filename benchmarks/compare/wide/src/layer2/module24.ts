import { computeL1M10, L1M10Kind, BaseL1M10 } from "../layer1/module10";

export enum L2M24Kind {
	Active,
	Idle,
	Done,
	Failed,
	Paused,
}

export const L2M24_FACTOR = 8;

export class BaseL2M24 {
	protected total = 49;
	public kind = L2M24Kind.Active;

	public shift(value: number): number {
		this.total += value * L2M24_FACTOR;
		return this.total;
	}

	public advance(): L2M24Kind {
		const order = [L2M24Kind.Active, L2M24Kind.Idle, L2M24Kind.Done, L2M24Kind.Failed, L2M24Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL2M24 extends BaseL1M10 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 11) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M24(input: number): number {
	let result = input * 8 + 49;
	result += computeL1M10(input % 10);
	return result % 68882;
}

export const L2M24_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 49,
	chain: (value) => computeL2M24(value),
};

export function linkL2M24(): Array<number> {
	const results = new Array<number>();
	const instanceL1M10 = new BaseL1M10();
	instanceL1M10.advance();
	results.push(instanceL1M10.read() + L1M10Kind.Idle);
	return results;
}
