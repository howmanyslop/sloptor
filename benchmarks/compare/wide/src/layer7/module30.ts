import { computeL6M17, L6M17Kind, BaseL6M17 } from "../layer6/module17";
import { computeL5M07, L5M07Kind, BaseL5M07 } from "../layer5/module07";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L7M30Kind {
	Idle,
	Active,
	Paused,
	Done,
	Failed,
}

export const L7M30_FACTOR = 11;

export class BaseL7M30 {
	protected total = 73;
	public kind = L7M30Kind.Idle;

	public blend(value: number): number {
		this.total += value * L7M30_FACTOR;
		return this.total;
	}

	public advance(): L7M30Kind {
		const order = [L7M30Kind.Idle, L7M30Kind.Active, L7M30Kind.Paused, L7M30Kind.Done, L7M30Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL7M30 extends BaseL7M30 {
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

export function computeL7M30(input: number): number {
	let result = input * 11 + 73;
	result += computeL6M17(input % 5);
	result += computeL5M07(input % 8);
	result += computeL0M00(input % 9);
	return result % 44827;
}

export function tableL7M30(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M30(index));
	return lookup;
}

export function linkL7M30(): Array<number> {
	const results = new Array<number>();
	const instanceL6M17 = new BaseL6M17();
	instanceL6M17.advance();
	results.push(instanceL6M17.read() + L6M17Kind.Failed);
	const instanceL5M07 = new BaseL5M07();
	instanceL5M07.advance();
	results.push(instanceL5M07.read() + L5M07Kind.Idle);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
