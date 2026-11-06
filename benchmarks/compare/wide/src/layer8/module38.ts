import { computeL7M34, L7M34Kind, BaseL7M34 } from "../layer7/module34";
import { computeL6M36, L6M36Kind, BaseL6M36 } from "../layer6/module36";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L8M38Kind {
	Idle,
	Active,
	Paused,
}

export const L8M38_FACTOR = 12;

export class BaseL8M38 {
	protected total = 30;
	public kind = L8M38Kind.Idle;

	public mix(value: number): number {
		this.total += value * L8M38_FACTOR;
		return this.total;
	}

	public advance(): L8M38Kind {
		const order = [L8M38Kind.Idle, L8M38Kind.Active, L8M38Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL8M38 extends BaseL7M34 {
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

export function computeL8M38(input: number): number {
	let result = input * 12 + 30;
	result += computeL7M34(input % 3);
	result += computeL6M36(input % 5);
	result += computeL0M03(input % 3);
	return result % 5590;
}

export const L8M38_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 30,
	chain: (value) => computeL8M38(value),
};

export function linkL8M38(): Array<number> {
	const results = new Array<number>();
	const instanceL7M34 = new BaseL7M34();
	instanceL7M34.advance();
	results.push(instanceL7M34.read() + L7M34Kind.Done);
	const instanceL6M36 = new BaseL6M36();
	instanceL6M36.advance();
	results.push(instanceL6M36.read() + L6M36Kind.Queued);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
