import { computeL4M30, L4M30Kind, BaseL4M30 } from "../layer4/module30";
import { computeL0M01, L0M01Kind, BaseL0M01 } from "../layer0/module01";

export enum L5M15Kind {
	Locked,
	Queued,
	Paused,
	Done,
	Active,
}

export const L5M15_FACTOR = 14;

export class BaseL5M15 {
	protected total = 84;
	public kind = L5M15Kind.Locked;

	public shift(value: number): number {
		this.total += value * L5M15_FACTOR;
		return this.total;
	}

	public advance(): L5M15Kind {
		const order = [L5M15Kind.Locked, L5M15Kind.Queued, L5M15Kind.Paused, L5M15Kind.Done, L5M15Kind.Active];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL5M15 extends BaseL4M30 {
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

export function computeL5M15(input: number): number {
	let result = input * 14 + 84;
	result += computeL4M30(input % 5);
	result += computeL0M01(input % 4);
	return result % 26718;
}

export function describeL5M15(kind: L5M15Kind): string {
	switch (kind) {
		case L5M15Kind.Locked:
			return "locked";
		case L5M15Kind.Queued:
			return "queued";
		case L5M15Kind.Paused:
			return "paused";
		case L5M15Kind.Done:
			return "done";
		case L5M15Kind.Active:
			return "active";
	}
}

export function linkL5M15(): Array<number> {
	const results = new Array<number>();
	const instanceL4M30 = new BaseL4M30();
	instanceL4M30.advance();
	results.push(instanceL4M30.read() + L4M30Kind.Idle);
	const instanceL0M01 = new BaseL0M01();
	instanceL0M01.advance();
	results.push(instanceL0M01.read() + L0M01Kind.Done);
	return results;
}
