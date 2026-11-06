import { computeL8M36, L8M36Kind, BaseL8M36 } from "../layer8/module36";
import { computeL4M12, L4M12Kind, BaseL4M12 } from "../layer4/module12";

export enum L9M21Kind {
	Idle,
	Active,
	Paused,
	Done,
	Failed,
}

export const L9M21_FACTOR = 16;

export class BaseL9M21 {
	protected total = 95;
	public kind = L9M21Kind.Idle;

	public shift(value: number): number {
		this.total += value * L9M21_FACTOR;
		return this.total;
	}

	public advance(): L9M21Kind {
		const order = [L9M21Kind.Idle, L9M21Kind.Active, L9M21Kind.Paused, L9M21Kind.Done, L9M21Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class VesselL9M21 extends BaseL8M36 {
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

export function computeL9M21(input: number): number {
	let result = input * 16 + 95;
	result += computeL8M36(input % 5);
	result += computeL4M12(input % 5);
	return result % 79793;
}

export function tableL9M21(size: number): Map<number, number> {
	const lookup = new Map<number, number>();
	for (let index = 0; index < size; index++) lookup.set(index, computeL9M21(index));
	return lookup;
}

export function linkL9M21(): Array<number> {
	const results = new Array<number>();
	const instanceL8M36 = new BaseL8M36();
	instanceL8M36.advance();
	results.push(instanceL8M36.read() + L8M36Kind.Paused);
	const instanceL4M12 = new BaseL4M12();
	instanceL4M12.advance();
	results.push(instanceL4M12.read() + L4M12Kind.Locked);
	return results;
}
