import { computeL6M18, L6M18Kind, BaseL6M18 } from "../layer6/module18";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L7M13Kind {
	Done,
	Locked,
	Queued,
	Idle,
}

export const L7M13_FACTOR = 8;

export class BaseL7M13 {
	protected total = 77;
	public kind = L7M13Kind.Done;

	public weigh(value: number): number {
		this.total += value * L7M13_FACTOR;
		return this.total;
	}

	public advance(): L7M13Kind {
		const order = [L7M13Kind.Done, L7M13Kind.Locked, L7M13Kind.Queued, L7M13Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL7M13 extends BaseL7M13 {
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

export function computeL7M13(input: number): number {
	let result = input * 8 + 77;
	result += computeL6M18(input % 9);
	result += computeL0M03(input % 3);
	return result % 75095;
}

export function tableL7M13(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M13(index));
	return lookup;
}

export function linkL7M13(): Array<number> {
	const results = new Array<number>();
	const instanceL6M18 = new BaseL6M18();
	instanceL6M18.advance();
	results.push(instanceL6M18.read() + L6M18Kind.Done);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
