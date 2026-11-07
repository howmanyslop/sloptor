import { computeL4M35, L4M35Kind, BaseL4M35 } from "../layer4/module35";
import { computeL1M07, L1M07Kind, BaseL1M07 } from "../layer1/module07";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L5M10Kind {
	Paused,
	Done,
	Failed,
	Queued,
}

export const L5M10_FACTOR = 4;

export class BaseL5M10 {
	protected total = 74;
	public kind = L5M10Kind.Paused;

	public mix(value: number): number {
		this.total += value * L5M10_FACTOR;
		return this.total;
	}

	public advance(): L5M10Kind {
		const order = [L5M10Kind.Paused, L5M10Kind.Done, L5M10Kind.Failed, L5M10Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL5M10 extends BaseL4M35 {
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

export function computeL5M10(input: number): number {
	let result = input * 4 + 74;
	result += computeL4M35(input % 9);
	result += computeL1M07(input % 11);
	result += computeL0M02(input % 3);
	return result % 57880;
}

export function tableL5M10(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL5M10(index));
	return lookup;
}

export function linkL5M10(): Array<number> {
	const results = new Array<number>();
	const instanceL4M35 = new BaseL4M35();
	instanceL4M35.advance();
	results.push(instanceL4M35.read() + L4M35Kind.Queued);
	const instanceL1M07 = new BaseL1M07();
	instanceL1M07.advance();
	results.push(instanceL1M07.read() + L1M07Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
