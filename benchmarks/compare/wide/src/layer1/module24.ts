import { computeL0M09, L0M09Kind, BaseL0M09 } from "../layer0/module09";
import { computeL0M02, L0M02Kind, BaseL0M02 } from "../layer0/module02";

export enum L1M24Kind {
	Active,
	Idle,
	Done,
	Failed,
	Queued,
}

export const L1M24_FACTOR = 10;

export class BaseL1M24 {
	protected total = 14;
	public kind = L1M24Kind.Active;

	public blend(value: number): number {
		this.total += value * L1M24_FACTOR;
		return this.total;
	}

	public advance(): L1M24Kind {
		const order = [L1M24Kind.Active, L1M24Kind.Idle, L1M24Kind.Done, L1M24Kind.Failed, L1M24Kind.Queued];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class GadgetL1M24 extends BaseL0M09 {
	private readonly history = new Array<number>();

	public record(value: number): void {
		this.history.push(value);
		if (this.history.size() > 6) this.history.shift();
	}

	public average(): number {
		let sum = 0;
		for (const value of this.history) sum += value;
		return this.history.size() > 0 ? sum / this.history.size() : this.read();
	}
}

export function computeL1M24(input: number): number {
	let result = input * 10 + 14;
	result += computeL0M09(input % 9);
	result += computeL0M02(input % 4);
	return result % 88196;
}

export function describeL1M24(kind: L1M24Kind): string {
	switch (kind) {
		case L1M24Kind.Active:
			return "active";
		case L1M24Kind.Idle:
			return "idle";
		case L1M24Kind.Done:
			return "done";
		case L1M24Kind.Failed:
			return "failed";
		case L1M24Kind.Queued:
			return "queued";
	}
}

export function linkL1M24(): Array<number> {
	const results = new Array<number>();
	const instanceL0M09 = new BaseL0M09();
	instanceL0M09.advance();
	results.push(instanceL0M09.read() + L0M09Kind.Failed);
	const instanceL0M02 = new BaseL0M02();
	instanceL0M02.advance();
	results.push(instanceL0M02.read() + L0M02Kind.Paused);
	return results;
}
