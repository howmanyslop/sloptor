import { computeL8M05, L8M05Kind, BaseL8M05 } from "../layer8/module05";
import { computeL5M16, L5M16Kind, BaseL5M16 } from "../layer5/module16";

export enum L9M28Kind {
	Done,
	Idle,
	Queued,
}

export const L9M28_FACTOR = 15;

export class BaseL9M28 {
	protected total = 33;
	public kind = L9M28Kind.Done;

	public fold(value: number): number {
		this.total += value * L9M28_FACTOR;
		return this.total;
	}

	public advance(): L9M28Kind {
		const order = [L9M28Kind.Done, L9M28Kind.Idle, L9M28Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL9M28 extends BaseL8M05 {
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

export function computeL9M28(input: number): number {
	let result = input * 15 + 33;
	result += computeL8M05(input % 5);
	result += computeL5M16(input % 3);
	return result % 93803;
}

export function tableL9M28(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M28(index));
	return lookup;
}

export function linkL9M28(): Array<number> {
	const results = new Array<number>();
	const instanceL8M05 = new BaseL8M05();
	instanceL8M05.advance();
	results.push(instanceL8M05.read() + L8M05Kind.Failed);
	const instanceL5M16 = new BaseL5M16();
	instanceL5M16.advance();
	results.push(instanceL5M16.read() + L5M16Kind.Done);
	return results;
}
