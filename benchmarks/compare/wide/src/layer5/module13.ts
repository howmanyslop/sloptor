import { computeL4M16, L4M16Kind, BaseL4M16 } from "../layer4/module16";

export enum L5M13Kind {
	Idle,
	Done,
	Queued,
	Active,
	Locked,
}

export const L5M13_FACTOR = 15;

export class BaseL5M13 {
	protected total = 8;
	public kind = L5M13Kind.Idle;

	public fold(value: number): number {
		this.total += value * L5M13_FACTOR;
		return this.total;
	}

	public advance(): L5M13Kind {
		const order = [L5M13Kind.Idle, L5M13Kind.Done, L5M13Kind.Queued, L5M13Kind.Active, L5M13Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL5M13 extends BaseL5M13 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 7) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL5M13(input: number): number {
	let result = input * 15 + 8;
	result += computeL4M16(input % 3);
	return result % 73351;
}

export const L5M13_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 8,
	chain: (value) => computeL5M13(value),
};

export function linkL5M13(): Array<number> {
	const results = new Array<number>();
	const instanceL4M16 = new BaseL4M16();
	instanceL4M16.advance();
	results.push(instanceL4M16.read() + L4M16Kind.Paused);
	return results;
}
