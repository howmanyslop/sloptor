import { computeL7M32, L7M32Kind, BaseL7M32 } from "../layer7/module32";
import { computeL4M19, L4M19Kind, BaseL4M19 } from "../layer4/module19";
import { computeL1M38, L1M38Kind, BaseL1M38 } from "../layer1/module38";

export enum L8M32Kind {
	Done,
	Paused,
	Active,
	Locked,
	Failed,
}

export const L8M32_FACTOR = 11;

export class BaseL8M32 {
	protected total = 70;
	public kind = L8M32Kind.Done;

	public scale(value: number): number {
		this.total += value * L8M32_FACTOR;
		return this.total;
	}

	public advance(): L8M32Kind {
		const order = [L8M32Kind.Done, L8M32Kind.Paused, L8M32Kind.Active, L8M32Kind.Locked, L8M32Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL8M32 extends BaseL7M32 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 8) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M32(input: number): number {
	let result = input * 11 + 70;
	result += computeL7M32(input % 5);
	result += computeL4M19(input % 10);
	result += computeL1M38(input % 7);
	return result % 64978;
}

export function tableL8M32(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M32(index));
	return lookup;
}

export function linkL8M32(): Array<number> {
	const results = new Array<number>();
	const instanceL7M32 = new BaseL7M32();
	instanceL7M32.advance();
	results.push(instanceL7M32.read() + L7M32Kind.Idle);
	const instanceL4M19 = new BaseL4M19();
	instanceL4M19.advance();
	results.push(instanceL4M19.read() + L4M19Kind.Locked);
	const instanceL1M38 = new BaseL1M38();
	instanceL1M38.advance();
	results.push(instanceL1M38.read() + L1M38Kind.Locked);
	return results;
}
