import { computeL1M08, L1M08Kind, BaseL1M08 } from "../layer1/module08";
import { computeL1M27, L1M27Kind, BaseL1M27 } from "../layer1/module27";
import { computeL1M05, L1M05Kind, BaseL1M05 } from "../layer1/module05";

export enum L2M18Kind {
	Done,
	Idle,
	Locked,
	Queued,
	Active,
}

export const L2M18_FACTOR = 11;

export class BaseL2M18 {
	protected total = 83;
	public kind = L2M18Kind.Done;

	public warp(value: number): number {
		this.total += value * L2M18_FACTOR;
		return this.total;
	}

	public advance(): L2M18Kind {
		const order = [L2M18Kind.Done, L2M18Kind.Idle, L2M18Kind.Locked, L2M18Kind.Queued, L2M18Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL2M18 extends BaseL1M08 {
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

export function computeL2M18(input: number): number {
	let result = input * 11 + 83;
	result += computeL1M08(input % 10);
	result += computeL1M27(input % 5);
	result += computeL1M05(input % 11);
	return result % 92938;
}

export function tableL2M18(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL2M18(index));
	return lookup;
}

export function linkL2M18(): Array<number> {
	const results = new Array<number>();
	const instanceL1M08 = new BaseL1M08();
	instanceL1M08.advance();
	results.push(instanceL1M08.read() + L1M08Kind.Active);
	const instanceL1M27 = new BaseL1M27();
	instanceL1M27.advance();
	results.push(instanceL1M27.read() + L1M27Kind.Paused);
	const instanceL1M05 = new BaseL1M05();
	instanceL1M05.advance();
	results.push(instanceL1M05.read() + L1M05Kind.Idle);
	return results;
}
