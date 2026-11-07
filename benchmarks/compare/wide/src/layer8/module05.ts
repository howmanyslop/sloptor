import { computeL7M03, L7M03Kind, BaseL7M03 } from "../layer7/module03";

export enum L8M05Kind {
	Failed,
	Active,
	Locked,
}

export const L8M05_FACTOR = 3;

export class BaseL8M05 {
	protected total = 2;
	public kind = L8M05Kind.Failed;

	public mix(value: number): number {
		this.total += value * L8M05_FACTOR;
		return this.total;
	}

	public advance(): L8M05Kind {
		const order = [L8M05Kind.Failed, L8M05Kind.Active, L8M05Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL8M05 extends BaseL8M05 {
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

export function computeL8M05(input: number): number {
	let result = input * 3 + 2;
	result += computeL7M03(input % 10);
	return result % 14068;
}

export function tableL8M05(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M05(index));
	return lookup;
}

export function linkL8M05(): Array<number> {
	const results = new Array<number>();
	const instanceL7M03 = new BaseL7M03();
	instanceL7M03.advance();
	results.push(instanceL7M03.read() + L7M03Kind.Queued);
	return results;
}
