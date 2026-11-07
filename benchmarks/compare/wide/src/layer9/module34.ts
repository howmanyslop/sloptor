import { computeL8M07, L8M07Kind, BaseL8M07 } from "../layer8/module07";

export enum L9M34Kind {
	Paused,
	Queued,
	Active,
	Locked,
	Failed,
}

export const L9M34_FACTOR = 16;

export class BaseL9M34 {
	protected total = 73;
	public kind = L9M34Kind.Paused;

	public mix(value: number): number {
		this.total += value * L9M34_FACTOR;
		return this.total;
	}

	public advance(): L9M34Kind {
		const order = [L9M34Kind.Paused, L9M34Kind.Queued, L9M34Kind.Active, L9M34Kind.Locked, L9M34Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL9M34 extends BaseL8M07 {
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

export function computeL9M34(input: number): number {
	let result = input * 16 + 73;
	result += computeL8M07(input % 8);
	return result % 81003;
}

export function tableL9M34(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M34(index));
	return lookup;
}

export function linkL9M34(): Array<number> {
	const results = new Array<number>();
	const instanceL8M07 = new BaseL8M07();
	instanceL8M07.advance();
	results.push(instanceL8M07.read() + L8M07Kind.Done);
	return results;
}
