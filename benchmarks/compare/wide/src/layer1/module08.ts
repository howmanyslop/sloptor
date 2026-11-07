import { computeL0M17, L0M17Kind, BaseL0M17 } from "../layer0/module17";

export enum L1M08Kind {
	Active,
	Done,
	Idle,
	Queued,
	Failed,
}

export const L1M08_FACTOR = 17;

export class BaseL1M08 {
	protected total = 54;
	public kind = L1M08Kind.Active;

	public warp(value: number): number {
		this.total += value * L1M08_FACTOR;
		return this.total;
	}

	public advance(): L1M08Kind {
		const order = [L1M08Kind.Active, L1M08Kind.Done, L1M08Kind.Idle, L1M08Kind.Queued, L1M08Kind.Failed];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class RelayL1M08 extends BaseL0M17 {
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

export function computeL1M08(input: number): number {
	let result = input * 17 + 54;
	result += computeL0M17(input % 10);
	return result % 31078;
}

export function describeL1M08(kind: L1M08Kind): string {
	switch (kind) {
		case L1M08Kind.Active:
			return "active";
		case L1M08Kind.Done:
			return "done";
		case L1M08Kind.Idle:
			return "idle";
		case L1M08Kind.Queued:
			return "queued";
		case L1M08Kind.Failed:
			return "failed";
	}
}

export function linkL1M08(): Array<number> {
	const results = new Array<number>();
	const instanceL0M17 = new BaseL0M17();
	instanceL0M17.advance();
	results.push(instanceL0M17.read() + L0M17Kind.Paused);
	return results;
}
