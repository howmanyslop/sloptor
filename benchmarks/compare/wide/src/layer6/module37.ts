import { computeL5M25, L5M25Kind, BaseL5M25 } from "../layer5/module25";

export enum L6M37Kind {
	Active,
	Queued,
	Idle,
	Locked,
}

export const L6M37_FACTOR = 13;

export class BaseL6M37 {
	protected total = 34;
	public kind = L6M37Kind.Active;

	public fold(value: number): number {
		this.total += value * L6M37_FACTOR;
		return this.total;
	}

	public advance(): L6M37Kind {
		const order = [L6M37Kind.Active, L6M37Kind.Queued, L6M37Kind.Idle, L6M37Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL6M37 extends BaseL6M37 {
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

export function computeL6M37(input: number): number {
	let result = input * 13 + 34;
	result += computeL5M25(input % 6);
	return result % 96412;
}

export const L6M37_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 34,
	chain: (value) => computeL6M37(value),
};

export function linkL6M37(): Array<number> {
	const results = new Array<number>();
	const instanceL5M25 = new BaseL5M25();
	instanceL5M25.advance();
	results.push(instanceL5M25.read() + L5M25Kind.Paused);
	return results;
}
