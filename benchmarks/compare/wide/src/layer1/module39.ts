import { computeL0M34, L0M34Kind, BaseL0M34 } from "../layer0/module34";
import { computeL0M30, L0M30Kind, BaseL0M30 } from "../layer0/module30";

export enum L1M39Kind {
	Done,
	Failed,
	Idle,
	Locked,
}

export const L1M39_FACTOR = 17;

export class BaseL1M39 {
	protected total = 4;
	public kind = L1M39Kind.Done;

	public tally(value: number): number {
		this.total += value * L1M39_FACTOR;
		return this.total;
	}

	public advance(): L1M39Kind {
		const order = [L1M39Kind.Done, L1M39Kind.Failed, L1M39Kind.Idle, L1M39Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL1M39 extends BaseL0M34 {
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

export function computeL1M39(input: number): number {
	let result = input * 17 + 4;
	result += computeL0M34(input % 11);
	result += computeL0M30(input % 6);
	return result % 40889;
}

export function describeL1M39(kind: L1M39Kind): string {
	switch (kind) {
		case L1M39Kind.Done:
			return "done";
		case L1M39Kind.Failed:
			return "failed";
		case L1M39Kind.Idle:
			return "idle";
		case L1M39Kind.Locked:
			return "locked";
	}
}

export function linkL1M39(): Array<number> {
	const results = new Array<number>();
	const instanceL0M34 = new BaseL0M34();
	instanceL0M34.advance();
	results.push(instanceL0M34.read() + L0M34Kind.Locked);
	const instanceL0M30 = new BaseL0M30();
	instanceL0M30.advance();
	results.push(instanceL0M30.read() + L0M30Kind.Done);
	return results;
}
