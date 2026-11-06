import { computeL4M07, L4M07Kind, BaseL4M07 } from "../layer4/module07";
import { computeL4M30, L4M30Kind, BaseL4M30 } from "../layer4/module30";
import { computeL4M14, L4M14Kind, BaseL4M14 } from "../layer4/module14";

export enum L5M06Kind {
	Done,
	Paused,
	Idle,
	Failed,
}

export const L5M06_FACTOR = 10;

export class BaseL5M06 {
	protected total = 93;
	public kind = L5M06Kind.Done;

	public scale(value: number): number {
		this.total += value * L5M06_FACTOR;
		return this.total;
	}

	public advance(): L5M06Kind {
		const order = [L5M06Kind.Done, L5M06Kind.Paused, L5M06Kind.Idle, L5M06Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL5M06 extends BaseL4M07 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 11) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL5M06(input: number): number {
	let result = input * 10 + 93;
	result += computeL4M07(input % 4);
	result += computeL4M30(input % 11);
	result += computeL4M14(input % 10);
	return result % 28817;
}

export function tableL5M06(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M06(index));
	return lookup;
}

export function linkL5M06(): Array<number> {
	const results = new Array<number>();
	const instanceL4M07 = new BaseL4M07();
	instanceL4M07.advance();
	results.push(instanceL4M07.read() + L4M07Kind.Paused);
	const instanceL4M30 = new BaseL4M30();
	instanceL4M30.advance();
	results.push(instanceL4M30.read() + L4M30Kind.Idle);
	const instanceL4M14 = new BaseL4M14();
	instanceL4M14.advance();
	results.push(instanceL4M14.read() + L4M14Kind.Idle);
	return results;
}
