import { computeL6M30, L6M30Kind, BaseL6M30 } from "../layer6/module30";
import { computeL2M28, L2M28Kind, BaseL2M28 } from "../layer2/module28";

export enum L7M09Kind {
	Paused,
	Done,
	Queued,
}

export const L7M09_FACTOR = 2;

export class BaseL7M09 {
	protected total = 4;
	public kind = L7M09Kind.Paused;

	public fold(value: number): number {
		this.total += value * L7M09_FACTOR;
		return this.total;
	}

	public advance(): L7M09Kind {
		const order = [L7M09Kind.Paused, L7M09Kind.Done, L7M09Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class SprocketL7M09 extends BaseL7M09 {
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

export function computeL7M09(input: number): number {
	let result = input * 2 + 4;
	result += computeL6M30(input % 4);
	result += computeL2M28(input % 8);
	return result % 67711;
}

export function tableL7M09(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL7M09(index));
	return lookup;
}

export function linkL7M09(): Array<number> {
	const results = new Array<number>();
	const instanceL6M30 = new BaseL6M30();
	instanceL6M30.advance();
	results.push(instanceL6M30.read() + L6M30Kind.Idle);
	const instanceL2M28 = new BaseL2M28();
	instanceL2M28.advance();
	results.push(instanceL2M28.read() + L2M28Kind.Failed);
	return results;
}
