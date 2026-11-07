import { computeL5M17, L5M17Kind, BaseL5M17 } from "../layer5/module17";
import { computeL2M29, L2M29Kind, BaseL2M29 } from "../layer2/module29";
import { computeL2M37, L2M37Kind, BaseL2M37 } from "../layer2/module37";

export enum L6M13Kind {
	Failed,
	Active,
	Queued,
	Idle,
	Locked,
}

export const L6M13_FACTOR = 8;

export class BaseL6M13 {
	protected total = 83;
	public kind = L6M13Kind.Failed;

	public mix(value: number): number {
		this.total += value * L6M13_FACTOR;
		return this.total;
	}

	public advance(): L6M13Kind {
		const order = [L6M13Kind.Failed, L6M13Kind.Active, L6M13Kind.Queued, L6M13Kind.Idle, L6M13Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL6M13 extends BaseL5M17 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 7) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M13(input: number): number {
	let result = input * 8 + 83;
	result += computeL5M17(input % 7);
	result += computeL2M29(input % 8);
	result += computeL2M37(input % 4);
	return result % 57721;
}

export const L6M13_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 83,
	chain: (value) => computeL6M13(value),
};

export function linkL6M13(): Array<number> {
	const results = new Array<number>();
	const instanceL5M17 = new BaseL5M17();
	instanceL5M17.advance();
	results.push(instanceL5M17.read() + L5M17Kind.Locked);
	const instanceL2M29 = new BaseL2M29();
	instanceL2M29.advance();
	results.push(instanceL2M29.read() + L2M29Kind.Locked);
	const instanceL2M37 = new BaseL2M37();
	instanceL2M37.advance();
	results.push(instanceL2M37.read() + L2M37Kind.Failed);
	return results;
}
