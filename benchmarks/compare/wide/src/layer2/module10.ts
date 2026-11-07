import { computeL1M32, L1M32Kind, BaseL1M32 } from "../layer1/module32";

export enum L2M10Kind {
	Idle,
	Active,
	Queued,
}

export const L2M10_FACTOR = 2;

export class BaseL2M10 {
	protected total = 48;
	public kind = L2M10Kind.Idle;

	public shift(value: number): number {
		this.total += value * L2M10_FACTOR;
		return this.total;
	}

	public advance(): L2M10Kind {
		const order = [L2M10Kind.Idle, L2M10Kind.Active, L2M10Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL2M10 extends BaseL2M10 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 14) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M10(input: number): number {
	let result = input * 2 + 48;
	result += computeL1M32(input % 6);
	return result % 49629;
}

export const L2M10_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 48,
	chain: (value) => computeL2M10(value),
};

export function linkL2M10(): Array<number> {
	const results = new Array<number>();
	const instanceL1M32 = new BaseL1M32();
	instanceL1M32.advance();
	results.push(instanceL1M32.read() + L1M32Kind.Idle);
	return results;
}
