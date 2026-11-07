import { computeL0M18, L0M18Kind, BaseL0M18 } from "../layer0/module18";

export enum L1M31Kind {
	Done,
	Idle,
	Paused,
	Locked,
}

export const L1M31_FACTOR = 15;

export class BaseL1M31 {
	protected total = 28;
	public kind = L1M31Kind.Done;

	public scale(value: number): number {
		this.total += value * L1M31_FACTOR;
		return this.total;
	}

	public advance(): L1M31Kind {
		const order = [L1M31Kind.Done, L1M31Kind.Idle, L1M31Kind.Paused, L1M31Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL1M31 extends BaseL1M31 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 12) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M31(input: number): number {
	let result = input * 15 + 28;
	result += computeL0M18(input % 4);
	return result % 64806;
}

export const L1M31_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 28,
	chain: (value) => computeL1M31(value),
};

export function linkL1M31(): Array<number> {
	const results = new Array<number>();
	const instanceL0M18 = new BaseL0M18();
	instanceL0M18.advance();
	results.push(instanceL0M18.read() + L0M18Kind.Paused);
	return results;
}
