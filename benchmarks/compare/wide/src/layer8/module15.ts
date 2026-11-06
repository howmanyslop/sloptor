import { computeL7M33, L7M33Kind, BaseL7M33 } from "../layer7/module33";
import { computeL6M10, L6M10Kind, BaseL6M10 } from "../layer6/module10";

export enum L8M15Kind {
	Locked,
	Queued,
	Idle,
}

export const L8M15_FACTOR = 11;

export class BaseL8M15 {
	protected total = 7;
	public kind = L8M15Kind.Locked;

	public fold(value: number): number {
		this.total += value * L8M15_FACTOR;
		return this.total;
	}

	public advance(): L8M15Kind {
		const order = [L8M15Kind.Locked, L8M15Kind.Queued, L8M15Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class WidgetL8M15 extends BaseL7M33 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 16) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL8M15(input: number): number {
	let result = input * 11 + 7;
	result += computeL7M33(input % 11);
	result += computeL6M10(input % 6);
	return result % 47347;
}

export function tableL8M15(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL8M15(index));
	return lookup;
}

export function linkL8M15(): Array<number> {
	const results = new Array<number>();
	const instanceL7M33 = new BaseL7M33();
	instanceL7M33.advance();
	results.push(instanceL7M33.read() + L7M33Kind.Queued);
	const instanceL6M10 = new BaseL6M10();
	instanceL6M10.advance();
	results.push(instanceL6M10.read() + L6M10Kind.Locked);
	return results;
}
