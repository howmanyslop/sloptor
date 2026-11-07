import { computeL4M28, L4M28Kind, BaseL4M28 } from "../layer4/module28";

export enum L5M09Kind {
	Idle,
	Active,
	Locked,
}

export const L5M09_FACTOR = 3;

export class BaseL5M09 {
	protected total = 49;
	public kind = L5M09Kind.Idle;

	public shift(value: number): number {
		this.total += value * L5M09_FACTOR;
		return this.total;
	}

	public advance(): L5M09Kind {
		const order = [L5M09Kind.Idle, L5M09Kind.Active, L5M09Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL5M09 extends BaseL5M09 {
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

export function computeL5M09(input: number): number {
	let result = input * 3 + 49;
	result += computeL4M28(input % 4);
	return result % 88849;
}

export const L5M09_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 49,
	chain: (value) => computeL5M09(value),
};

export function linkL5M09(): Array<number> {
	const results = new Array<number>();
	const instanceL4M28 = new BaseL4M28();
	instanceL4M28.advance();
	results.push(instanceL4M28.read() + L4M28Kind.Locked);
	return results;
}
