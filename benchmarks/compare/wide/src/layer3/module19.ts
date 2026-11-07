import { computeL2M16, L2M16Kind, BaseL2M16 } from "../layer2/module16";
import { computeL0M11, L0M11Kind, BaseL0M11 } from "../layer0/module11";
import { computeL2M06, L2M06Kind, BaseL2M06 } from "../layer2/module06";

export enum L3M19Kind {
	Active,
	Failed,
	Queued,
}

export const L3M19_FACTOR = 5;

export class BaseL3M19 {
	protected total = 4;
	public kind = L3M19Kind.Active;

	public shift(value: number): number {
		this.total += value * L3M19_FACTOR;
		return this.total;
	}

	public advance(): L3M19Kind {
		const order = [L3M19Kind.Active, L3M19Kind.Failed, L3M19Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL3M19 extends BaseL3M19 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 9) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M19(input: number): number {
	let result = input * 5 + 4;
	result += computeL2M16(input % 4);
	result += computeL0M11(input % 5);
	result += computeL2M06(input % 3);
	return result % 13808;
}

export const L3M19_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 4,
	chain: (value) => computeL3M19(value),
};

export function linkL3M19(): Array<number> {
	const results = new Array<number>();
	const instanceL2M16 = new BaseL2M16();
	instanceL2M16.advance();
	results.push(instanceL2M16.read() + L2M16Kind.Paused);
	const instanceL0M11 = new BaseL0M11();
	instanceL0M11.advance();
	results.push(instanceL0M11.read() + L0M11Kind.Active);
	const instanceL2M06 = new BaseL2M06();
	instanceL2M06.advance();
	results.push(instanceL2M06.read() + L2M06Kind.Active);
	return results;
}
