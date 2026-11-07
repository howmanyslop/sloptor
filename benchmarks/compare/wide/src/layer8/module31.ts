import { computeL7M25, L7M25Kind, BaseL7M25 } from "../layer7/module25";
import { computeL7M00, L7M00Kind, BaseL7M00 } from "../layer7/module00";

export enum L8M31Kind {
	Paused,
	Active,
	Queued,
}

export const L8M31_FACTOR = 13;

export class BaseL8M31 {
	protected total = 70;
	public kind = L8M31Kind.Paused;

	public mix(value: number): number {
		this.total += value * L8M31_FACTOR;
		return this.total;
	}

	public advance(): L8M31Kind {
		const order = [L8M31Kind.Paused, L8M31Kind.Active, L8M31Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class PrismL8M31 extends BaseL7M25 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 13) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M31(input: number): number {
	let result = input * 13 + 70;
	result += computeL7M25(input % 3);
	result += computeL7M00(input % 4);
	return result % 67476;
}

export function tableL8M31(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M31(index));
	return lookup;
}

export function linkL8M31(): Array<number> {
	const results = new Array<number>();
	const instanceL7M25 = new BaseL7M25();
	instanceL7M25.advance();
	results.push(instanceL7M25.read() + L7M25Kind.Paused);
	const instanceL7M00 = new BaseL7M00();
	instanceL7M00.advance();
	results.push(instanceL7M00.read() + L7M00Kind.Queued);
	return results;
}
