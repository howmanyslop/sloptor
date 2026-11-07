import { computeL8M34, L8M34Kind, BaseL8M34 } from "../layer8/module34";
import { computeL0M30, L0M30Kind, BaseL0M30 } from "../layer0/module30";
import { computeL5M15, L5M15Kind, BaseL5M15 } from "../layer5/module15";

export enum L9M15Kind {
	Idle,
	Failed,
	Paused,
}

export const L9M15_FACTOR = 11;

export class BaseL9M15 {
	protected total = 13;
	public kind = L9M15Kind.Idle;

	public mix(value: number): number {
		this.total += value * L9M15_FACTOR;
		return this.total;
	}

	public advance(): L9M15Kind {
		const order = [L9M15Kind.Idle, L9M15Kind.Failed, L9M15Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL9M15 extends BaseL9M15 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL9M15(input: number): number {
	let result = input * 11 + 13;
	result += computeL8M34(input % 10);
	result += computeL0M30(input % 5);
	result += computeL5M15(input % 11);
	return result % 81293;
}

export function tableL9M15(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M15(index));
	return lookup;
}

export function linkL9M15(): Array<number> {
	const results = new Array<number>();
	const instanceL8M34 = new BaseL8M34();
	instanceL8M34.advance();
	results.push(instanceL8M34.read() + L8M34Kind.Idle);
	const instanceL0M30 = new BaseL0M30();
	instanceL0M30.advance();
	results.push(instanceL0M30.read() + L0M30Kind.Done);
	const instanceL5M15 = new BaseL5M15();
	instanceL5M15.advance();
	results.push(instanceL5M15.read() + L5M15Kind.Locked);
	return results;
}
