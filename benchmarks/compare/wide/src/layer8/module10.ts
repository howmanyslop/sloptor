import { computeL7M38, L7M38Kind, BaseL7M38 } from "../layer7/module38";
import { computeL6M17, L6M17Kind, BaseL6M17 } from "../layer6/module17";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L8M10Kind {
	Active,
	Locked,
	Paused,
}

export const L8M10_FACTOR = 3;

export class BaseL8M10 {
	protected total = 84;
	public kind = L8M10Kind.Active;

	public shift(value: number): number {
		this.total += value * L8M10_FACTOR;
		return this.total;
	}

	public advance(): L8M10Kind {
		const order = [L8M10Kind.Active, L8M10Kind.Locked, L8M10Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL8M10 extends BaseL7M38 {
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

export function computeL8M10(input: number): number {
	let result = input * 3 + 84;
	result += computeL7M38(input % 11);
	result += computeL6M17(input % 4);
	result += computeL0M02(input % 10);
	return result % 87077;
}

export const L8M10_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 84,
	chain: (value) => computeL8M10(value),
};

export function linkL8M10(): Array<number> {
	const results = new Array<number>();
	const instanceL7M38 = new BaseL7M38();
	instanceL7M38.advance();
	results.push(instanceL7M38.read() + L7M38Kind.Locked);
	const instanceL6M17 = new BaseL6M17();
	instanceL6M17.advance();
	results.push(instanceL6M17.read() + L6M17Kind.Failed);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
