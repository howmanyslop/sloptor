import { computeL8M38, L8M38Kind, BaseL8M38 } from "../layer8/module38";
import { computeL1M22, L1M22Kind, BaseL1M22 } from "../layer1/module22";
import { computeL0M11, L0M11Kind, BaseL0M11 } from "../layer0/module11";

export enum L9M27Kind {
	Locked,
	Active,
	Queued,
	Paused,
}

export const L9M27_FACTOR = 5;

export class BaseL9M27 {
	protected total = 3;
	public kind = L9M27Kind.Locked;

	public blend(value: number): number {
		this.total += value * L9M27_FACTOR;
		return this.total;
	}

	public advance(): L9M27Kind {
		const order = [L9M27Kind.Locked, L9M27Kind.Active, L9M27Kind.Queued, L9M27Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL9M27 extends BaseL9M27 {
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

export function computeL9M27(input: number): number {
	let result = input * 5 + 3;
	result += computeL8M38(input % 7);
	result += computeL1M22(input % 6);
	result += computeL0M11(input % 11);
	return result % 69843;
}

export const L9M27_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 3,
	chain: (value) => computeL9M27(value),
};

export function linkL9M27(): Array<number> {
	const results = new Array<number>();
	const instanceL8M38 = new BaseL8M38();
	instanceL8M38.advance();
	results.push(instanceL8M38.read() + L8M38Kind.Idle);
	const instanceL1M22 = new BaseL1M22();
	instanceL1M22.advance();
	results.push(instanceL1M22.read() + L1M22Kind.Idle);
	const instanceL0M11 = new BaseL0M11();
	instanceL0M11.advance();
	results.push(instanceL0M11.read() + L0M11Kind.Active);
	return results;
}
