import { computeL3M18, L3M18Kind, BaseL3M18 } from "../layer3/module18";
import { computeL2M26, L2M26Kind, BaseL2M26 } from "../layer2/module26";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L4M02Kind {
	Queued,
	Failed,
	Done,
}

export const L4M02_FACTOR = 17;

export class BaseL4M02 {
	protected total = 58;
	public kind = L4M02Kind.Queued;

	public fold(value: number): number {
		this.total += value * L4M02_FACTOR;
		return this.total;
	}

	public advance(): L4M02Kind {
		const order = [L4M02Kind.Queued, L4M02Kind.Failed, L4M02Kind.Done];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL4M02 extends BaseL3M18 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 5) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL4M02(input: number): number {
	let result = input * 17 + 58;
	result += computeL3M18(input % 10);
	result += computeL2M26(input % 9);
	result += computeL0M02(input % 5);
	return result % 25669;
}

export function tableL4M02(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL4M02(index));
	return lookup;
}

export function linkL4M02(): Array<number> {
	const results = new Array<number>();
	const instanceL3M18 = new BaseL3M18();
	instanceL3M18.advance();
	results.push(instanceL3M18.read() + L3M18Kind.Idle);
	const instanceL2M26 = new BaseL2M26();
	instanceL2M26.advance();
	results.push(instanceL2M26.read() + L2M26Kind.Idle);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
