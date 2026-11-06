import { computeL5M04, L5M04Kind, BaseL5M04 } from "../layer5/module04";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L6M34Kind {
	Idle,
	Locked,
	Queued,
}

export const L6M34_FACTOR = 12;

export class BaseL6M34 {
	protected total = 17;
	public kind = L6M34Kind.Idle;

	public warp(value: number): number {
		this.total += value * L6M34_FACTOR;
		return this.total;
	}

	public advance(): L6M34Kind {
		const order = [L6M34Kind.Idle, L6M34Kind.Locked, L6M34Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL6M34 extends BaseL5M04 {
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

export function computeL6M34(input: number): number {
	let result = input * 12 + 17;
	result += computeL5M04(input % 8);
	result += computeL0M03(input % 4);
	return result % 14165;
}

export function tableL6M34(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL6M34(index));
	return lookup;
}

export function linkL6M34(): Array<number> {
	const results = new Array<number>();
	const instanceL5M04 = new BaseL5M04();
	instanceL5M04.advance();
	results.push(instanceL5M04.read() + L5M04Kind.Active);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
