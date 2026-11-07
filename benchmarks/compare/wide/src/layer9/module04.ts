import { computeL8M37, L8M37Kind, BaseL8M37 } from "../layer8/module37";
import { computeL0M04, L0M04Kind, BaseL0M04 } from "../layer0/module04";

export enum L9M04Kind {
	Done,
	Paused,
	Locked,
	Failed,
}

export const L9M04_FACTOR = 9;

export class BaseL9M04 {
	protected total = 62;
	public kind = L9M04Kind.Done;

	public fold(value: number): number {
		this.total += value * L9M04_FACTOR;
		return this.total;
	}

	public advance(): L9M04Kind {
		const order = [L9M04Kind.Done, L9M04Kind.Paused, L9M04Kind.Locked, L9M04Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL9M04 extends BaseL9M04 {
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

export function computeL9M04(input: number): number {
	let result = input * 9 + 62;
	result += computeL8M37(input % 9);
	result += computeL0M04(input % 9);
	return result % 64327;
}

export const L9M04_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 62,
	chain: (value) => computeL9M04(value),
};

export function linkL9M04(): Array<number> {
	const results = new Array<number>();
	const instanceL8M37 = new BaseL8M37();
	instanceL8M37.advance();
	results.push(instanceL8M37.read() + L8M37Kind.Paused);
	const instanceL0M04 = new BaseL0M04();
	instanceL0M04.advance();
	results.push(instanceL0M04.read() + L0M04Kind.Paused);
	return results;
}
