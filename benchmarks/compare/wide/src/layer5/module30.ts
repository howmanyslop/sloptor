import { computeL4M15, L4M15Kind, BaseL4M15 } from "../layer4/module15";
import { computeL3M17, L3M17Kind, BaseL3M17 } from "../layer3/module17";

export enum L5M30Kind {
	Idle,
	Locked,
	Paused,
	Queued,
	Done,
}

export const L5M30_FACTOR = 10;

export class BaseL5M30 {
	protected total = 26;
	public kind = L5M30Kind.Idle;

	public scale(value: number): number {
		this.total += value * L5M30_FACTOR;
		return this.total;
	}

	public advance(): L5M30Kind {
		const order = [L5M30Kind.Idle, L5M30Kind.Locked, L5M30Kind.Paused, L5M30Kind.Queued, L5M30Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL5M30 extends BaseL4M15 {
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

export function computeL5M30(input: number): number {
	let result = input * 10 + 26;
	result += computeL4M15(input % 9);
	result += computeL3M17(input % 8);
	return result % 55175;
}

export function tableL5M30(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M30(index));
	return lookup;
}

export function linkL5M30(): Array<number> {
	const results = new Array<number>();
	const instanceL4M15 = new BaseL4M15();
	instanceL4M15.advance();
	results.push(instanceL4M15.read() + L4M15Kind.Paused);
	const instanceL3M17 = new BaseL3M17();
	instanceL3M17.advance();
	results.push(instanceL3M17.read() + L3M17Kind.Done);
	return results;
}
