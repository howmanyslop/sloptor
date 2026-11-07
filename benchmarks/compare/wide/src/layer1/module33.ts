import { computeL0M32, L0M32Kind, BaseL0M32 } from "../layer0/module32";
import { computeL0M28, L0M28Kind, BaseL0M28 } from "../layer0/module28";
import { computeL0M05, L0M05Kind, BaseL0M05 } from "../layer0/module05";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L1M33Kind {
	Idle,
	Paused,
	Done,
	Failed,
}

export const L1M33_FACTOR = 15;

export class BaseL1M33 {
	protected total = 62;
	public kind = L1M33Kind.Idle;

	public fold(value: number): number {
		this.total += value * L1M33_FACTOR;
		return this.total;
	}

	public advance(): L1M33Kind {
		const order = [L1M33Kind.Idle, L1M33Kind.Paused, L1M33Kind.Done, L1M33Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL1M33 extends BaseL0M32 {
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

export function computeL1M33(input: number): number {
	let result = input * 15 + 62;
	result += computeL0M32(input % 9);
	result += computeL0M28(input % 9);
	result += computeL0M05(input % 10);
	result += computeL0M02(input % 4);
	return result % 34262;
}

export function tableL1M33(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL1M33(index));
	return lookup;
}

export function linkL1M33(): Array<number> {
	const results = new Array<number>();
	const instanceL0M32 = new BaseL0M32();
	instanceL0M32.advance();
	results.push(instanceL0M32.read() + L0M32Kind.Paused);
	const instanceL0M28 = new BaseL0M28();
	instanceL0M28.advance();
	results.push(instanceL0M28.read() + L0M28Kind.Active);
	const instanceL0M05 = new BaseL0M05();
	instanceL0M05.advance();
	results.push(instanceL0M05.read() + L0M05Kind.Done);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
