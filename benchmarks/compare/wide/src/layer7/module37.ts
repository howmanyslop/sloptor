import { computeL6M26, L6M26Kind, BaseL6M26 } from "../layer6/module26";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L7M37Kind {
	Queued,
	Failed,
	Done,
}

export const L7M37_FACTOR = 10;

export class BaseL7M37 {
	protected total = 15;
	public kind = L7M37Kind.Queued;

	public shift(value: number): number {
		this.total += value * L7M37_FACTOR;
		return this.total;
	}

	public advance(): L7M37Kind {
		const order = [L7M37Kind.Queued, L7M37Kind.Failed, L7M37Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL7M37 extends BaseL6M26 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M37(input: number): number {
	let result = input * 10 + 15;
	result += computeL6M26(input % 8);
	result += computeL0M00(input % 5);
	return result % 46933;
}

export function tableL7M37(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M37(index));
	return lookup;
}

export function linkL7M37(): Array<number> {
	const results = new Array<number>();
	const instanceL6M26 = new BaseL6M26();
	instanceL6M26.advance();
	results.push(instanceL6M26.read() + L6M26Kind.Locked);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
