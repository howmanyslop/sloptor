import { computeL4M11, L4M11Kind, BaseL4M11 } from "../layer4/module11";

export enum L5M18Kind {
	Locked,
	Done,
	Active,
	Queued,
	Idle,
}

export const L5M18_FACTOR = 7;

export class BaseL5M18 {
	protected total = 24;
	public kind = L5M18Kind.Locked;

	public mix(value: number): number {
		this.total += value * L5M18_FACTOR;
		return this.total;
	}

	public advance(): L5M18Kind {
		const order = [L5M18Kind.Locked, L5M18Kind.Done, L5M18Kind.Active, L5M18Kind.Queued, L5M18Kind.Idle];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LatticeL5M18 extends BaseL5M18 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 4) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL5M18(input: number): number {
	let result = input * 7 + 24;
	result += computeL4M11(input % 3);
	return result % 9364;
}

export const L5M18_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 24,
	chain: (value) => computeL5M18(value),
};

export function linkL5M18(): Array<number> {
	const results = new Array<number>();
	const instanceL4M11 = new BaseL4M11();
	instanceL4M11.advance();
	results.push(instanceL4M11.read() + L4M11Kind.Done);
	return results;
}
