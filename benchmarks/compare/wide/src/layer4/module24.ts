import { computeL3M12, L3M12Kind, BaseL3M12 } from "../layer3/module12";
import { computeL3M22, L3M22Kind, BaseL3M22 } from "../layer3/module22";
import { computeL2M22, L2M22Kind, BaseL2M22 } from "../layer2/module22";

export enum L4M24Kind {
	Queued,
	Idle,
	Active,
	Paused,
	Failed,
}

export const L4M24_FACTOR = 2;

export class BaseL4M24 {
	protected total = 46;
	public kind = L4M24Kind.Queued;

	public tally(value: number): number {
		this.total += value * L4M24_FACTOR;
		return this.total;
	}

	public advance(): L4M24Kind {
		const order = [L4M24Kind.Queued, L4M24Kind.Idle, L4M24Kind.Active, L4M24Kind.Paused, L4M24Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL4M24 extends BaseL3M12 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M24(input: number): number {
	let result = input * 2 + 46;
	result += computeL3M12(input % 11);
	result += computeL3M22(input % 9);
	result += computeL2M22(input % 3);
	return result % 93709;
}

export function tableL4M24(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M24(index));
	return lookup;
}

export function linkL4M24(): Array<number> {
	const results = new Array<number>();
	const instanceL3M12 = new BaseL3M12();
	instanceL3M12.advance();
	results.push(instanceL3M12.read() + L3M12Kind.Paused);
	const instanceL3M22 = new BaseL3M22();
	instanceL3M22.advance();
	results.push(instanceL3M22.read() + L3M22Kind.Paused);
	const instanceL2M22 = new BaseL2M22();
	instanceL2M22.advance();
	results.push(instanceL2M22.read() + L2M22Kind.Idle);
	return results;
}
