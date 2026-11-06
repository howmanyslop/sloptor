import { computeL5M38, L5M38Kind, BaseL5M38 } from "../layer5/module38";
import { computeL2M14, L2M14Kind, BaseL2M14 } from "../layer2/module14";

export enum L6M16Kind {
	Idle,
	Done,
	Active,
}

export const L6M16_FACTOR = 7;

export class BaseL6M16 {
	protected total = 25;
	public kind = L6M16Kind.Idle;

	public blend(value: number): number {
		this.total += value * L6M16_FACTOR;
		return this.total;
	}

	public advance(): L6M16Kind {
		const order = [L6M16Kind.Idle, L6M16Kind.Done, L6M16Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL6M16 extends BaseL5M38 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL6M16(input: number): number {
	let result = input * 7 + 25;
	result += computeL5M38(input % 6);
	result += computeL2M14(input % 5);
	return result % 17241;
}

export function tableL6M16(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M16(index));
	return lookup;
}

export function linkL6M16(): Array<number> {
	const results = new Array<number>();
	const instanceL5M38 = new BaseL5M38();
	instanceL5M38.advance();
	results.push(instanceL5M38.read() + L5M38Kind.Paused);
	const instanceL2M14 = new BaseL2M14();
	instanceL2M14.advance();
	results.push(instanceL2M14.read() + L2M14Kind.Queued);
	return results;
}
