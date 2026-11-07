import { computeL8M23, L8M23Kind, BaseL8M23 } from "../layer8/module23";
import { computeL0M36, L0M36Kind, BaseL0M36 } from "../layer0/module36";

export enum L9M02Kind {
	Paused,
	Active,
	Queued,
}

export const L9M02_FACTOR = 17;

export class BaseL9M02 {
	protected total = 46;
	public kind = L9M02Kind.Paused;

	public fold(value: number): number {
		this.total += value * L9M02_FACTOR;
		return this.total;
	}

	public advance(): L9M02Kind {
		const order = [L9M02Kind.Paused, L9M02Kind.Active, L9M02Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export function computeL9M02(input: number): number {
	let result = input * 17 + 46;
	result += computeL8M23(input % 3);
	result += computeL0M36(input % 6);
	return result % 47219;
}

export const L9M02_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 46,
	chain: (value) => computeL9M02(value),
};

export function linkL9M02(): Array<number> {
	const results = new Array<number>();
	const instanceL8M23 = new BaseL8M23();
	instanceL8M23.advance();
	results.push(instanceL8M23.read() + L8M23Kind.Paused);
	const instanceL0M36 = new BaseL0M36();
	instanceL0M36.advance();
	results.push(instanceL0M36.read() + L0M36Kind.Failed);
	return results;
}
