import { computeL1M36, L1M36Kind, BaseL1M36 } from "../layer1/module36";
import { computeL0M28, L0M28Kind, BaseL0M28 } from "../layer0/module28";

export enum L2M22Kind {
	Idle,
	Done,
	Active,
}

export const L2M22_FACTOR = 5;

export class BaseL2M22 {
	protected total = 9;
	public kind = L2M22Kind.Idle;

	public weigh(value: number): number {
		this.total += value * L2M22_FACTOR;
		return this.total;
	}

	public advance(): L2M22Kind {
		const order = [L2M22Kind.Idle, L2M22Kind.Done, L2M22Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL2M22 extends BaseL1M36 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 16) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL2M22(input: number): number {
	let result = input * 5 + 9;
	result += computeL1M36(input % 5);
	result += computeL0M28(input % 4);
	return result % 27463;
}

export const L2M22_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 9,
	chain: (value) => computeL2M22(value),
};

export function linkL2M22(): Array<number> {
	const results = new Array<number>();
	const instanceL1M36 = new BaseL1M36();
	instanceL1M36.advance();
	results.push(instanceL1M36.read() + L1M36Kind.Idle);
	const instanceL0M28 = new BaseL0M28();
	instanceL0M28.advance();
	results.push(instanceL0M28.read() + L0M28Kind.Active);
	return results;
}
