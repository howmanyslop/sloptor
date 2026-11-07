import { computeL7M17, L7M17Kind, BaseL7M17 } from "../layer7/module17";
import { computeL3M05, L3M05Kind, BaseL3M05 } from "../layer3/module05";

export enum L8M07Kind {
	Done,
	Idle,
	Queued,
	Paused,
	Locked,
}

export const L8M07_FACTOR = 15;

export class BaseL8M07 {
	protected total = 55;
	public kind = L8M07Kind.Done;

	public weigh(value: number): number {
		this.total += value * L8M07_FACTOR;
		return this.total;
	}

	public advance(): L8M07Kind {
		const order = [L8M07Kind.Done, L8M07Kind.Idle, L8M07Kind.Queued, L8M07Kind.Paused, L8M07Kind.Locked];
		const position = order.indexOf(this.kind);
		this.kind = order[(position + 1) % order.size()];
		return this.kind;
	}

	public read(): number {
		return this.total;
	}
}

export class LedgerL8M07 extends BaseL7M17 {
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

export function computeL8M07(input: number): number {
	let result = input * 15 + 55;
	result += computeL7M17(input % 6);
	result += computeL3M05(input % 10);
	return result % 23887;
}

export function describeL8M07(kind: L8M07Kind): string {
	switch (kind) {
		case L8M07Kind.Done:
			return "done";
		case L8M07Kind.Idle:
			return "idle";
		case L8M07Kind.Queued:
			return "queued";
		case L8M07Kind.Paused:
			return "paused";
		case L8M07Kind.Locked:
			return "locked";
	}
}

export function linkL8M07(): Array<number> {
	const results = new Array<number>();
	const instanceL7M17 = new BaseL7M17();
	instanceL7M17.advance();
	results.push(instanceL7M17.read() + L7M17Kind.Locked);
	const instanceL3M05 = new BaseL3M05();
	instanceL3M05.advance();
	results.push(instanceL3M05.read() + L3M05Kind.Failed);
	return results;
}
