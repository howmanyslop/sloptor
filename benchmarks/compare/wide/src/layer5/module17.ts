import { computeL4M04, L4M04Kind, BaseL4M04 } from "../layer4/module04";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L5M17Kind {
	Locked,
	Queued,
	Failed,
	Done,
}

export const L5M17_FACTOR = 7;

export class BaseL5M17 {
	protected total = 20;
	public kind = L5M17Kind.Locked;

	public shift(value: number): number {
		this.total += value * L5M17_FACTOR;
		return this.total;
	}

	public advance(): L5M17Kind {
		const order = [L5M17Kind.Locked, L5M17Kind.Queued, L5M17Kind.Failed, L5M17Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL5M17 extends BaseL5M17 {
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

export function computeL5M17(input: number): number {
	let result = input * 7 + 20;
	result += computeL4M04(input % 9);
	result += computeL0M00(input % 10);
	return result % 68047;
}

export const L5M17_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 20,
	chain: (value) => computeL5M17(value),
};

export function linkL5M17(): Array<number> {
	const results = new Array<number>();
	const instanceL4M04 = new BaseL4M04();
	instanceL4M04.advance();
	results.push(instanceL4M04.read() + L4M04Kind.Paused);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
