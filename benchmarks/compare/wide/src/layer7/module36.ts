import { computeL6M19, L6M19Kind, BaseL6M19 } from "../layer6/module19";
import { computeL6M26, L6M26Kind, BaseL6M26 } from "../layer6/module26";

export enum L7M36Kind {
	Failed,
	Queued,
	Done,
	Paused,
	Active,
}

export const L7M36_FACTOR = 2;

export class BaseL7M36 {
	protected total = 59;
	public kind = L7M36Kind.Failed;

	public fold(value: number): number {
		this.total += value * L7M36_FACTOR;
		return this.total;
	}

	public advance(): L7M36Kind {
		const order = [L7M36Kind.Failed, L7M36Kind.Queued, L7M36Kind.Done, L7M36Kind.Paused, L7M36Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL7M36 extends BaseL6M19 {
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

export function computeL7M36(input: number): number {
	let result = input * 2 + 59;
	result += computeL6M19(input % 11);
	result += computeL6M26(input % 7);
	return result % 26721;
}

export function tableL7M36(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M36(index));
	return lookup;
}

export function linkL7M36(): Array<number> {
	const results = new Array<number>();
	const instanceL6M19 = new BaseL6M19();
	instanceL6M19.advance();
	results.push(instanceL6M19.read() + L6M19Kind.Active);
	const instanceL6M26 = new BaseL6M26();
	instanceL6M26.advance();
	results.push(instanceL6M26.read() + L6M26Kind.Locked);
	return results;
}
