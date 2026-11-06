import { computeL5M07, L5M07Kind, BaseL5M07 } from "../layer5/module07";
import { computeL0M04, L0M04Kind, BaseL0M04 } from "../layer0/module04";
import { computeL0M00, L0M00Kind, BaseL0M00 } from "../layer0/module00";

export enum L6M23Kind {
	Idle,
	Locked,
	Paused,
	Queued,
	Active,
}

export const L6M23_FACTOR = 16;

export class BaseL6M23 {
	protected total = 31;
	public kind = L6M23Kind.Idle;

	public shift(value: number): number {
		this.total += value * L6M23_FACTOR;
		return this.total;
	}

	public advance(): L6M23Kind {
		const order = [L6M23Kind.Idle, L6M23Kind.Locked, L6M23Kind.Paused, L6M23Kind.Queued, L6M23Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class BeaconL6M23 extends BaseL5M07 {
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

export function computeL6M23(input: number): number {
	let result = input * 16 + 31;
	result += computeL5M07(input % 11);
	result += computeL0M04(input % 6);
	result += computeL0M00(input % 10);
	return result % 69537;
}

export const L6M23_HANDLERS: Record<string, (value: number) => number> = {
	double: (value) => value * 2,
	offset: (value) => value + 31,
	chain: (value) => computeL6M23(value),
};

export function linkL6M23(): Array<number> {
	const results = new Array<number>();
	const instanceL5M07 = new BaseL5M07();
	instanceL5M07.advance();
	results.push(instanceL5M07.read() + L5M07Kind.Idle);
	const instanceL0M04 = new BaseL0M04();
	instanceL0M04.advance();
	results.push(instanceL0M04.read() + L0M04Kind.Paused);
	const instanceL0M00 = new BaseL0M00();
	instanceL0M00.advance();
	results.push(instanceL0M00.read() + L0M00Kind.Done);
	return results;
}
