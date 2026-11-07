import { computeL7M37, L7M37Kind, BaseL7M37 } from "../layer7/module37";
import { computeL7M25, L7M25Kind, BaseL7M25 } from "../layer7/module25";

export enum L8M27Kind {
	Paused,
	Done,
	Failed,
	Locked,
}

export const L8M27_FACTOR = 13;

export class BaseL8M27 {
	protected total = 37;
	public kind = L8M27Kind.Paused;

	public warp(value: number): number {
		this.total += value * L8M27_FACTOR;
		return this.total;
	}

	public advance(): L8M27Kind {
		const order = [L8M27Kind.Paused, L8M27Kind.Done, L8M27Kind.Failed, L8M27Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL8M27 extends BaseL8M27 {
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

export function computeL8M27(input: number): number {
	let result = input * 13 + 37;
	result += computeL7M37(input % 10);
	result += computeL7M25(input % 8);
	return result % 79011;
}

export function tableL8M27(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M27(index));
	return lookup;
}

export function linkL8M27(): Array<number> {
	const results = new Array<number>();
	const instanceL7M37 = new BaseL7M37();
	instanceL7M37.advance();
	results.push(instanceL7M37.read() + L7M37Kind.Queued);
	const instanceL7M25 = new BaseL7M25();
	instanceL7M25.advance();
	results.push(instanceL7M25.read() + L7M25Kind.Paused);
	return results;
}
