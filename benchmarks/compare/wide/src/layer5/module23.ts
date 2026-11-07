import { computeL4M06, L4M06Kind, BaseL4M06 } from "../layer4/module06";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L5M23Kind {
	Paused,
	Active,
	Idle,
	Failed,
}

export const L5M23_FACTOR = 7;

export class BaseL5M23 {
	protected total = 67;
	public kind = L5M23Kind.Paused;

	public blend(value: number): number {
		this.total += value * L5M23_FACTOR;
		return this.total;
	}

	public advance(): L5M23Kind {
		const order = [L5M23Kind.Paused, L5M23Kind.Active, L5M23Kind.Idle, L5M23Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL5M23 extends BaseL4M06 {
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

export function computeL5M23(input: number): number {
	let result = input * 7 + 67;
	result += computeL4M06(input % 8);
	result += computeL0M03(input % 5);
	return result % 45480;
}

export function tableL5M23(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M23(index));
	return lookup;
}

export function linkL5M23(): Array<number> {
	const results = new Array<number>();
	const instanceL4M06 = new BaseL4M06();
	instanceL4M06.advance();
	results.push(instanceL4M06.read() + L4M06Kind.Paused);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
