import { computeL6M08, L6M08Kind, BaseL6M08 } from "../layer6/module08";
import { computeL0M31, L0M31Kind, BaseL0M31 } from "../layer0/module31";

export enum L7M23Kind {
	Paused,
	Done,
	Failed,
	Queued,
}

export const L7M23_FACTOR = 12;

export class BaseL7M23 {
	protected total = 91;
	public kind = L7M23Kind.Paused;

	public warp(value: number): number {
		this.total += value * L7M23_FACTOR;
		return this.total;
	}

	public advance(): L7M23Kind {
		const order = [L7M23Kind.Paused, L7M23Kind.Done, L7M23Kind.Failed, L7M23Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL7M23 extends BaseL7M23 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 14) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL7M23(input: number): number {
	let result = input * 12 + 91;
	result += computeL6M08(input % 7);
	result += computeL0M31(input % 4);
	return result % 35896;
}

export function tableL7M23(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M23(index));
	return lookup;
}

export function linkL7M23(): Array<number> {
	const results = new Array<number>();
	const instanceL6M08 = new BaseL6M08();
	instanceL6M08.advance();
	results.push(instanceL6M08.read() + L6M08Kind.Failed);
	const instanceL0M31 = new BaseL0M31();
	instanceL0M31.advance();
	results.push(instanceL0M31.read() + L0M31Kind.Failed);
	return results;
}
