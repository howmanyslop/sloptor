import { computeL2M29, L2M29Kind, BaseL2M29 } from "../layer2/module29";

export enum L3M38Kind {
	Active,
	Paused,
	Done,
	Failed,
}

export const L3M38_FACTOR = 7;

export class BaseL3M38 {
	protected total = 48;
	public kind = L3M38Kind.Active;

	public mix(value: number): number {
		this.total += value * L3M38_FACTOR;
		return this.total;
	}

	public advance(): L3M38Kind {
		const order = [L3M38Kind.Active, L3M38Kind.Paused, L3M38Kind.Done, L3M38Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL3M38 extends BaseL3M38 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M38(input: number): number {
	let result = input * 7 + 48;
	result += computeL2M29(input % 3);
	return result % 5035;
}

export function tableL3M38(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M38(index));
	return lookup;
}

export function linkL3M38(): Array<number> {
	const results = new Array<number>();
	const instanceL2M29 = new BaseL2M29();
	instanceL2M29.advance();
	results.push(instanceL2M29.read() + L2M29Kind.Locked);
	return results;
}
