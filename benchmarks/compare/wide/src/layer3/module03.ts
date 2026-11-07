import { computeL2M24, L2M24Kind, BaseL2M24 } from "../layer2/module24";

export enum L3M03Kind {
	Idle,
	Active,
	Paused,
	Done,
	Failed,
}

export const L3M03_FACTOR = 17;

export class BaseL3M03 {
	protected total = 16;
	public kind = L3M03Kind.Idle;

	public mix(value: number): number {
		this.total += value * L3M03_FACTOR;
		return this.total;
	}

	public advance(): L3M03Kind {
		const order = [L3M03Kind.Idle, L3M03Kind.Active, L3M03Kind.Paused, L3M03Kind.Done, L3M03Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL3M03 extends BaseL3M03 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 10) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL3M03(input: number): number {
	let result = input * 17 + 16;
	result += computeL2M24(input % 10);
	return result % 93344;
}

export function tableL3M03(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL3M03(index));
	return lookup;
}

export function linkL3M03(): Array<number> {
	const results = new Array<number>();
	const instanceL2M24 = new BaseL2M24();
	instanceL2M24.advance();
	results.push(instanceL2M24.read() + L2M24Kind.Active);
	return results;
}
