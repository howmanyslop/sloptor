import { computeL6M00, L6M00Kind, BaseL6M00 } from "../layer6/module00";
import { computeL5M09, L5M09Kind, BaseL5M09 } from "../layer5/module09";
import { computeL4M18, L4M18Kind, BaseL4M18 } from "../layer4/module18";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L7M39Kind {
	Queued,
	Failed,
	Done,
}

export const L7M39_FACTOR = 16;

export class BaseL7M39 {
	protected total = 64;
	public kind = L7M39Kind.Queued;

	public weigh(value: number): number {
		this.total += value * L7M39_FACTOR;
		return this.total;
	}

	public advance(): L7M39Kind {
		const order = [L7M39Kind.Queued, L7M39Kind.Failed, L7M39Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL7M39 extends BaseL6M00 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 9) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M39(input: number): number {
	let result = input * 16 + 64;
	result += computeL6M00(input % 10);
	result += computeL5M09(input % 3);
	result += computeL4M18(input % 6);
	result += computeL0M01(input % 10);
	return result % 63913;
}

export const L7M39_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 64,
	chain: (value) => computeL7M39(value),
};

export function linkL7M39(): Array<number> {
	const results = new Array<number>();
	const instanceL6M00 = new BaseL6M00();
	instanceL6M00.advance();
	results.push(instanceL6M00.read() + L6M00Kind.Idle);
	const instanceL5M09 = new BaseL5M09();
	instanceL5M09.advance();
	results.push(instanceL5M09.read() + L5M09Kind.Idle);
	const instanceL4M18 = new BaseL4M18();
	instanceL4M18.advance();
	results.push(instanceL4M18.read() + L4M18Kind.Paused);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
