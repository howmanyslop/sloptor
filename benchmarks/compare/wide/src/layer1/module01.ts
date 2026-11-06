import { computeL0M08, L0M08Kind, BaseL0M08 } from "../layer0/module08";
import { computeL0M03, L0M03Kind, BaseL0M03 } from "../layer0/module03";

export enum L1M01Kind {
	Locked,
	Idle,
	Active,
	Failed,
	Paused,
}

export const L1M01_FACTOR = 4;

export class BaseL1M01 {
	protected total = 51;
	public kind = L1M01Kind.Locked;

	public scale(value: number): number {
		this.total += value * L1M01_FACTOR;
		return this.total;
	}

	public advance(): L1M01Kind {
		const order = [L1M01Kind.Locked, L1M01Kind.Idle, L1M01Kind.Active, L1M01Kind.Failed, L1M01Kind.Paused];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL1M01 extends BaseL0M08 {
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

export function computeL1M01(input: number): number {
	let result = input * 4 + 51;
	result += computeL0M08(input % 9);
	result += computeL0M03(input % 3);
	return result % 91646;
}

export function describeL1M01(kind: L1M01Kind): string {
	switch (kind) {
		case L1M01Kind.Locked:
			return "locked";
		case L1M01Kind.Idle:
			return "idle";
		case L1M01Kind.Active:
			return "active";
		case L1M01Kind.Failed:
			return "failed";
		case L1M01Kind.Paused:
			return "paused";
	}
}

export function linkL1M01(): Array<number> {
	const results = new Array<number>();
	const instanceL0M08 = new BaseL0M08();
	instanceL0M08.advance();
	results.push(instanceL0M08.read() + L0M08Kind.Done);
	const instanceL0M03 = new BaseL0M03();
	instanceL0M03.advance();
	results.push(instanceL0M03.read() + L0M03Kind.Paused);
	return results;
}
