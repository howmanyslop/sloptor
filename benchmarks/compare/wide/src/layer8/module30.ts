import { computeL7M18, L7M18Kind, BaseL7M18 } from "../layer7/module18";

export enum L8M30Kind {
	Done,
	Queued,
	Active,
}

export const L8M30_FACTOR = 13;

export class BaseL8M30 {
	protected total = 32;
	public kind = L8M30Kind.Done;

	public mix(value: number): number {
		this.total += value * L8M30_FACTOR;
		return this.total;
	}

	public advance(): L8M30Kind {
		const order = [L8M30Kind.Done, L8M30Kind.Queued, L8M30Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL8M30 extends BaseL7M18 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 15) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M30(input: number): number {
	let result = input * 13 + 32;
	result += computeL7M18(input % 7);
	return result % 74048;
}

export function tableL8M30(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M30(index));
	return lookup;
}

export function linkL8M30(): Array<number> {
	const results = new Array<number>();
	const instanceL7M18 = new BaseL7M18();
	instanceL7M18.advance();
	results.push(instanceL7M18.read() + L7M18Kind.Idle);
	return results;
}
