import { computeL5M16, L5M16Kind, BaseL5M16 } from "../layer5/module16";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L6M30Kind {
	Idle,
	Active,
	Queued,
	Done,
	Locked,
}

export const L6M30_FACTOR = 5;

export class BaseL6M30 {
	protected total = 53;
	public kind = L6M30Kind.Idle;

	public tally(value: number): number {
		this.total += value * L6M30_FACTOR;
		return this.total;
	}

	public advance(): L6M30Kind {
		const order = [L6M30Kind.Idle, L6M30Kind.Active, L6M30Kind.Queued, L6M30Kind.Done, L6M30Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class AnchorL6M30 extends BaseL6M30 {
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

export function computeL6M30(input: number): number {
	let result = input * 5 + 53;
	result += computeL5M16(input % 3);
	result += computeL0M02(input % 7);
	return result % 78742;
}

export const L6M30_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 53,
	chain: (value) => computeL6M30(value),
};

export function linkL6M30(): Array<number> {
	const results = new Array<number>();
	const instanceL5M16 = new BaseL5M16();
	instanceL5M16.advance();
	results.push(instanceL5M16.read() + L5M16Kind.Done);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
