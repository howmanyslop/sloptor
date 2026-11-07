import { computeL2M22, L2M22Kind, BaseL2M22 } from "../layer2/module22";

export enum L3M37Kind {
	Failed,
	Done,
	Queued,
	Idle,
	Active,
}

export const L3M37_FACTOR = 4;

export class BaseL3M37 {
	protected total = 94;
	public kind = L3M37Kind.Failed;

	public fold(value: number): number {
		this.total += value * L3M37_FACTOR;
		return this.total;
	}

	public advance(): L3M37Kind {
		const order = [L3M37Kind.Failed, L3M37Kind.Done, L3M37Kind.Queued, L3M37Kind.Idle, L3M37Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL3M37 extends BaseL2M22 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M37(input: number): number {
	let result = input * 4 + 94;
	result += computeL2M22(input % 11);
	return result % 14880;
}

export const L3M37_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 94,
	chain: (value) => computeL3M37(value),
};

export function linkL3M37(): Array<number> {
	const results = new Array<number>();
	const instanceL2M22 = new BaseL2M22();
	instanceL2M22.advance();
	results.push(instanceL2M22.read() + L2M22Kind.Idle);
	return results;
}
